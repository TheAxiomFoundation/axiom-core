# Axiom core prototype

Use the actual Axiom Rust engine pinned in Cargo.toml. Do not implement policy
semantics in this repository's adapters, Python client, tests, or demos.

This checkpoint contains unsigned development bundles. Do not describe hash
verification as authentication, legal validation, signed admission, or a
production-ready service. Preserve those distinctions in CLI output and docs.

The native Rust engine request/response types are authoritative. Reject lost
request fields and preserve every response field. Avoid parallel hand-written
Python models. No raw ProgramSpec execution is exposed through the facade.

Run cargo fmt --all --check, cargo clippy --workspace --all-targets -- -D warnings,
cargo test --workspace, cargo build --workspace, and the Python transport tests
before handoff. All policy-like fixtures must be labeled synthetic and executed
by the pinned engine. Keep source snapshots and expected digests explicit.
