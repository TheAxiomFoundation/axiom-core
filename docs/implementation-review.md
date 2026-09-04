# Axiom core implementation review

Reviewed `/Users/maxghenis/TheAxiomFoundation/axiom-core` on the unborn `build/bundle-execution` branch, with uncommitted initial files, as explicitly requested. Read `AGENTS.md`, README, prototype contract, all four Rust source files, the Python transport/tests, and the pinned native engine request types, execution path, and dataset binder in the frozen runtime source. No implementation files were changed by this reviewer.

Scope is unsigned, trusted-source, local development bundles. Deferred signing, publication/services, legal validation, hostile-program isolation, and native scalar type/arithmetic guarantees are not acceptance requirements for this checkpoint.

## Review outcome

**No unresolved actionable findings after the size-limit fix and targeted re-review.** The one initial P2 finding below is retained with its reproduction and verified resolution.

## Resolved finding

### P2 — `build` can successfully emit a bundle that `verify` and `run` refuse to read

[main.rs:78](/Users/maxghenis/TheAxiomFoundation/axiom-core/crates/axiom-core/src/main.rs:78) compiles an accepted build spec, serializes the complete bundle, and writes it with exclusive creation at lines 85–92, without checking its serialized size. Every later file input is capped at 16 MiB by [read_text](/Users/maxghenis/TheAxiomFoundation/axiom-core/crates/axiom-core/src/main.rs:16). The bundle contains the source strings plus manifest and executable, so an accepted source spec can expand beyond that cap.

Reproduced using the actual CLI and ordinary two-module synthetic fixture, padded with a source comment. No policy calculation was reimplemented:

```json
{
  "input_bytes": 16776215,
  "limit": 16777216,
  "build_returncode": 0,
  "bundle_bytes": 16780496,
  "verify_returncode": 1,
  "verify_error_code": "input_too_large"
}
```

This violates the ordinary build→verify/run round trip: the builder returns an identity and success for an artifact unusable by the same binary. It is an availability/contract defect, not an integrity bypass; verification correctly rejects the oversized file.

Recommended fix: ensure final serialized bundle bytes, including the trailing newline, fit the supported bundle-reader limit before opening the destination. Alternatively introduce an explicitly compatible larger bundle cap. Add a test that a near-limit accepted spec either produces a loadable bundle or returns a structured error without creating an unusable object.

The implementing parent added the size check at [main.rs:81](/Users/maxghenis/TheAxiomFoundation/axiom-core/crates/axiom-core/src/main.rs:81), before destination creation. `bytes.len() >= MAX_INPUT_BYTES` correctly includes the one-byte newline in the reader limit. I inspected that change and independently ran the new real-CLI `test_build_rejects_unreadable_large_bundle_before_creating_file`: **passed**, asserting `bundle_too_large` for a spec of exactly `16 MiB - 1` bytes and no destination file. The reviewed fixed `main.rs` SHA-256 is `44eb312b57e111ae1a00a2f29eb930724ccade1e5d1fc4a6a41481a2e60b98d2`.

## Areas reviewed without additional actionable findings

- **Source/executable relation:** [bundle.rs:161](/Users/maxghenis/TheAxiomFoundation/axiom-core/crates/axiom-core/src/bundle.rs:161) checks the independent expected manifest digest, assurance/format, exact engine identity and compiler options, every source digest, the closure digest, and exact artifact bytes. It invokes the native validated artifact loader, recompiles only the supplied resolved source closure, and compares canonical executable bytes. The public execution handle has private fields and no bypass constructor.
- **Executable identity:** [lib.rs:26](/Users/maxghenis/TheAxiomFoundation/axiom-core/crates/axiom-core/src/lib.rs:26) includes the pinned native version/revision, artifact version, embedded dependency lockfile bytes, and streaming hash of the current executable; missing/unreadable binaries fail closed. [execution.rs:106](/Users/maxghenis/TheAxiomFoundation/axiom-core/crates/axiom-core/src/execution.rs:106) rechecks the host identity before execution. The documented trusted-host assumption correctly excludes replacing the on-disk host under the running process.
- **Native field preservation:** [execution.rs:59](/Users/maxghenis/TheAxiomFoundation/axiom-core/crates/axiom-core/src/execution.rs:59) checks duplicate JSON keys before map conversion, uses native typed deserialization with unknown-field collection, then closes known tagged-scalar/flattened-period gaps. The explicit guards cover the current pinned request schema. Assessment-date keys, including null, are rejected before they can imply unsupported semantics. Interval checks cover queries, inputs, and relations.
- **Scenario/context identity:** Duplicate pins are rejected and their execution/hash order normalized. Context includes bundle/artifact/host/wire identities, native queries/mode, and explicit scenario, including an empty scenario. Dataset omission is an explicit private-context contract, not a missing promised result identity. Native pin execution clones the verified artifact; it does not mutate stored baseline bytes.
- **Strict dataset preflight:** The facade runs the native strict binder over the unchanged input catalog before calling the native execution API. Pinning preserves native static input dependencies, so this preflight does not currently bind against a different input catalog.
- **Response preservation:** Receipts own the full native `ExecutionResponse`; Python forwards the JSON object and returns the complete Rust result without a mirrored calculation/schema model. Actual rounding details, parameter reads, metadata, and explanation traces reach Python unchanged.
- **CLI file behavior:** Exclusive output creation preserves existing bundles. No shell is involved in Python invocation. Requests travel via stdin, avoiding request values in process arguments. The documented partial-write and resource-limit limitations match implementation scope.

## Verification and residual risks

- Real Python CLI integration suite: **8 tests passed**, including native pin effects, valid-time behavior, complete receipts/traces, unknown fields, assessment dates, tampering and wrong expected digest.
- Executed the oversized-build reproduction against the same real binary; temporary source/bundle files were removed afterward.
- Independently ran `cargo test --locked --workspace`: **21 tests passed**, including the newly added rehashed-source/old-executable rejection. `cargo build --locked --workspace` also passed. Independently reran the new real-CLI size regression after the fix: **1 test passed**. The implementing parent separately runs all required final formatting, clippy and full transport checks; this reviewer does not substitute these focused results for those final checks.
- Schema guards are intentionally coupled to the pinned native schema. Future engine upgrades must review tagged/flattened variants and preservation tests together. This is a maintenance dependency, not a current blocker.
- Hash verification is local integrity against a caller-selected digest, not source-author authentication or legal correctness. Trusted input and original executable/request availability remain required, as documented.
