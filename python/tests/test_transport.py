"""Integration tests against the real CLI and synthetic engine fixtures.

Build the Rust binary first, then run:
    python3 -m unittest discover -s python/tests -v
No calculation mocks or Python policy implementations are used.
"""

from __future__ import annotations

import copy
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "python"))

from axiom_core import AxiomCoreError, build, capabilities, execute, verify  # noqa: E402


class RustTransportIntegrationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.binary = os.environ.get("AXIOM_CORE_BIN", str(REPO / "target/debug/axiom-core"))
        cls.scratch = tempfile.TemporaryDirectory(prefix="axiom-core-python-")
        cls.addClassCleanup(cls.scratch.cleanup)
        cls.bundle = Path(cls.scratch.name) / "synthetic.bundle.json"
        cls.built = build(REPO / "fixtures/synthetic-household.json", cls.bundle, binary=cls.binary)
        cls.digest = cls.built["bundle_sha256"]
        cls.request_2026 = json.loads((REPO / "fixtures/request-2026.json").read_text())
        cls.request_2027 = json.loads((REPO / "fixtures/request-2027.json").read_text())

    def cli(self, *arguments, request=None):
        return subprocess.run(
            [self.binary, *map(str, arguments)],
            input=None if request is None else json.dumps(request, allow_nan=False),
            capture_output=True, text=True, encoding="utf-8", timeout=30,
            check=False,
        )

    def direct_receipt(self, request):
        completed = self.cli("run", "--bundle", self.bundle, "--expect", self.digest, request=request)
        self.assertEqual(completed.returncode, 0, completed.stderr)
        return json.loads(completed.stdout)

    def receipt(self, request):
        return execute(self.bundle, self.digest, request, binary=self.binary)

    def test_complete_cli_receipt_is_preserved(self):
        request = copy.deepcopy(self.request_2026)
        before = copy.deepcopy(request)
        direct = self.direct_receipt(request)
        python = self.receipt(request)
        self.assertEqual(python, direct)
        self.assertEqual(request, before, "the transport must not mutate caller input")
        self.assertEqual(python["result"], direct["result"])
        traces = [query.get("trace", {}) for query in python["result"]["results"]]
        self.assertTrue(any(traces), "synthetic explain fixture must produce actual trace details")
        benefit = next(node for trace in traces for node in trace.values() if node["name"] == "benefit")
        for field in ("entity_id", "rounding", "pre_rounding_value", "executed_expression", "parameter_reads"):
            self.assertIn(field, benefit, f"real engine trace field {field} must reach Python intact")
        self.assertIn("metadata", python["result"])
        self.assertIn("context_sha256", python)
        self.assertIn("assurance", python)

    def test_native_pin_is_forwarded_and_changes_actual_execution(self):
        baseline = self.receipt(self.request_2026)
        pinned_request = copy.deepcopy(self.request_2026)
        pinned_request["pins"] = [{"rule": "benefit", "value": {"kind": "decimal", "value": "0"}}]
        pinned = self.receipt(pinned_request)
        self.assertEqual(pinned, self.direct_receipt(pinned_request))
        output_id = self.request_2026["queries"][0]["outputs"][0]
        baseline_value = baseline["result"]["results"][0]["outputs"][output_id]["value"]["value"]
        pinned_value = pinned["result"]["results"][0]["outputs"][output_id]["value"]["value"]
        self.assertNotEqual(str(baseline_value), "0")
        self.assertEqual(str(pinned_value), "0")
        self.assertNotEqual(baseline["context_sha256"], pinned["context_sha256"])

    def test_requested_period_reaches_engine(self):
        in_2026 = self.receipt(self.request_2026)
        in_2027 = self.receipt(self.request_2027)
        self.assertEqual(in_2027, self.direct_receipt(self.request_2027))
        self.assertNotEqual(
            in_2026["result"]["results"][0]["outputs"],
            in_2027["result"]["results"][0]["outputs"],
        )

    def test_empty_scenario_matches_baseline_and_execution_preserves_bundle(self):
        original_bundle = self.bundle.read_bytes()
        baseline = self.receipt(self.request_2026)
        explicit_empty = copy.deepcopy(self.request_2026)
        explicit_empty["pins"] = []
        self.assertEqual(baseline, self.receipt(explicit_empty))
        pinned = copy.deepcopy(self.request_2026)
        pinned["pins"] = [{"rule": "benefit", "value": {"kind": "decimal", "value": "0"}}]
        self.receipt(pinned)
        self.assertEqual(original_bundle, self.bundle.read_bytes())

    def test_period_outside_source_coverage_fails_without_fallback(self):
        request = copy.deepcopy(self.request_2027)
        request["dataset"]["inputs"][0]["interval"] = {"start": "2028-01-01", "end": "2028-01-31"}
        request["queries"][0]["period"].update(start="2028-01-01", end="2028-01-31")
        self.assert_rejected_like_cli(request)

    def assert_rejected_like_cli(self, request):
        completed = self.cli("run", "--bundle", self.bundle, "--expect", self.digest, request=request)
        self.assertNotEqual(completed.returncode, 0)
        expected = json.loads(completed.stderr)
        with self.assertRaises(AxiomCoreError) as caught:
            self.receipt(request)
        self.assertEqual(caught.exception.response, expected)
        self.assertEqual(caught.exception.code, expected["error"]["code"])
        self.assertEqual(caught.exception.returncode, completed.returncode)

    def test_unknown_fields_are_forwarded_for_rust_to_reject(self):
        request = copy.deepcopy(self.request_2026)
        request["silently_lost_field"] = True
        self.assert_rejected_like_cli(request)
        nested = copy.deepcopy(self.request_2026)
        nested["queries"][0]["period"]["silently_lost_field"] = True
        self.assert_rejected_like_cli(nested)

    def test_assessment_date_is_not_silently_ignored(self):
        for value in ("2026-01-01", None):
            with self.subTest(assessment_date=value):
                request = copy.deepcopy(self.request_2026)
                request["queries"][0]["assessment_date"] = value
                self.assert_rejected_like_cli(request)

    def test_tampered_bundle_is_rejected_before_execution(self):
        tampered = Path(self.scratch.name) / "tampered.bundle.json"
        body = json.loads(self.bundle.read_text())
        # Keep the bundle and executable valid JSON: this must fail on the
        # executable's exact byte digest, not just reject an unknown field.
        body["artifact_json"] += " "
        tampered.write_text(json.dumps(body))
        with self.assertRaises(AxiomCoreError) as caught:
            execute(tampered, self.digest, self.request_2026, binary=self.binary)
        self.assertEqual(caught.exception.code, "artifact_digest_mismatch")
        with self.assertRaises(AxiomCoreError):
            verify(tampered, self.digest, binary=self.binary)

    def test_wrong_expected_digest_is_rejected(self):
        with self.assertRaises(AxiomCoreError):
            execute(self.bundle, "0" * 64, self.request_2026, binary=self.binary)

    def test_build_rejects_unreadable_large_bundle_before_creating_file(self):
        spec = json.loads((REPO / "fixtures/synthetic-household.json").read_text())
        spec["modules"][spec["root"]] += "\n# "
        limit = 16 * 1024 * 1024
        padding = limit - 1 - len(json.dumps(spec).encode("utf-8"))
        spec["modules"][spec["root"]] += "x" * padding
        source = Path(self.scratch.name) / "large-spec.json"
        destination = Path(self.scratch.name) / "large.bundle.json"
        source.write_text(json.dumps(spec), encoding="utf-8")
        self.assertEqual(source.stat().st_size, limit - 1)
        with self.assertRaises(AxiomCoreError) as caught:
            build(source, destination, binary=self.binary)
        self.assertEqual(caught.exception.code, "bundle_too_large")
        self.assertFalse(destination.exists())

    def test_verify_and_capabilities_preserve_cli_objects(self):
        verified = self.cli("verify", "--bundle", self.bundle, "--expect", self.digest)
        self.assertEqual(verified.returncode, 0, verified.stderr)
        self.assertEqual(verify(self.bundle, self.digest, binary=self.binary), json.loads(verified.stdout))
        caps = self.cli("capabilities")
        self.assertEqual(caps.returncode, 0, caps.stderr)
        self.assertEqual(capabilities(binary=self.binary), json.loads(caps.stdout))


if __name__ == "__main__":
    unittest.main()
