# Python transport

This standard-library adapter invokes the real `axiom-core` Rust CLI. The pinned
engine owns request validation, policy semantics, and the complete execution
response. The adapter sends JSON through stdin and returns the entire receipt;
it does not maintain parallel request or response models.

Build the CLI with `cargo build --workspace`, then set `PYTHONPATH=python` from
the repository root:

```python
import json
from axiom_core import build, execute

identity = build("fixtures/synthetic-household.json", "/tmp/synthetic.bundle.json")
with open("fixtures/request-2026.json") as request_file:
    request = json.load(request_file)
receipt = execute("/tmp/synthetic.bundle.json", identity["bundle_sha256"], request)
```

An explicit `binary=` argument overrides `AXIOM_CORE_BIN`; otherwise the checkout's
`target/debug/axiom-core` is used. `timeout=` bounds each CLI subprocess in seconds.
`AxiomCoreError` retains the structured error in `.response`, together with `.code`,
`.message`, `.returncode`, and `.stderr`.

These are **unsigned development bundles**. Digest verification establishes
integrity against the supplied digest; it does not authenticate a publisher,
validate law, or establish production readiness. The example fixtures are
synthetic and execute through the pinned Axiom engine.

Run the integration tests after building the actual CLI:

```sh
python3 -m unittest discover -s python/tests -v
```
