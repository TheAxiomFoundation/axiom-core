"""Compile and execute a synthetic fixture through the actual Axiom CLI."""

from __future__ import annotations

import copy
import json
from pathlib import Path
import shutil
import sys
import tempfile

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "python"))

from axiom_core import build, execute, verify  # noqa: E402


def save(path: Path, value: dict) -> None:
    with path.open("x", encoding="utf-8") as stream:
        json.dump(value, stream, indent=2, allow_nan=False)
        stream.write("\n")


def main() -> None:
    output_root = REPO / "output"
    output_root.mkdir(exist_ok=True)
    # Fresh, private directory: no prior bundle or receipt is overwritten.
    output = Path(tempfile.mkdtemp(prefix="synthetic-", dir=output_root))
    binary = output / "axiom-core"
    # Preserve the exact executable needed to replay this development bundle.
    shutil.copy2(REPO / "target/debug/axiom-core", binary)
    bundle = output / "synthetic.bundle.json"
    identity = build(REPO / "fixtures/synthetic-household.json", bundle, binary=binary)
    save(output / "identity.json", identity)
    verify(bundle, identity["bundle_sha256"], binary=binary)

    requests = {
        year: json.loads((REPO / f"fixtures/request-{year}.json").read_text())
        for year in ("2026", "2027")
    }
    pinned = copy.deepcopy(requests["2026"])
    pinned["pins"] = [{"rule": "benefit", "value": {"kind": "decimal", "value": "0"}}]
    requests["2026-pinned"] = pinned

    print("Synthetic fixture — real Axiom engine, unsigned development bundle")
    print(f"Bundle: {identity['bundle_sha256']}")
    for name, request in requests.items():
        save(output / f"request-{name}.json", request)
        receipt = execute(bundle, identity["bundle_sha256"], request, binary=binary)
        save(output / f"receipt-{name}.json", receipt)
        # Display the engine's values verbatim; no calculation takes place here.
        outputs = receipt["result"]["results"][0]["outputs"]
        print(f"{name}: {json.dumps(outputs, sort_keys=True)}")
    print(f"Bundle, requests and receipts: {output}")


if __name__ == "__main__":
    main()
