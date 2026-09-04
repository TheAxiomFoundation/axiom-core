# Fable review of the Axiom proposal

Model: `claude-fable-5-1` · Subfleet run: `20260904-175824-axiom-architecture-proposal`

The substantive review below is preserved verbatim from reviewer message `b6567630-d7ed-4b36-8324-d7c7012c1db9`. Subfleet's final-output file contained a later acknowledgment of an unrelated routing notification; the review was recovered from this run's task transcript.

---

**Verification limits first.** I read the frozen proposal, the four review documents, and the engine snapshot's Cargo manifest, request schema, and CLI. I had no shell, so I could not recompute the SHA-256 you quoted. Nothing was edited or posted.

## Findings, by severity

**1. Blocker. Execution identity is named but never defined** (§3 line 66, §7 lines 114 and 116). The proposal lists what a response "identifies" but never fixes a canonical object with a hash rule. The engine's pin mechanism rewrites the program before execution (`api.rs:29-39`), so a pinned run executes different IR while loading the same artifact bytes. §7 allows two paths, specialization or runtime parameters, without saying which the first build uses. Failure: two responses carry the same artifact hash and different values, and a client that strips unknown fields drops the scenario field, which is exactly the Python defect the review reproduced. Revision: define one struct, returned by the executor, that is hash(bundle manifest, artifact bytes, engine build, wire schema version, canonical scenario, query period). Make the scenario hash required. An empty scenario has a defined hash and must produce an identity equal to the baseline.

**2. Blocker. Knowledge time is under-specified against the proposal's own compatibility rule** (§5 lines 90-92 versus §4 line 84). The engine echoes `assessment_date` without applying it (`api.rs:60-73`). §4 says required new semantics must fail compatibility rather than be discarded. §5 says echoing "does not claim to implement it." Both cannot hold. Failure: a caller sends date K, receives it echoed, and treats the result as what an administrator could have known. Revision: the strict executor rejects any request carrying an assessment date until bitemporal selection exists, and the bundle manifest's supported-semantics list omits it. This is one line to decide before building.

**3. Blocker. Several compiler promises cannot be kept by the current engine or by admission** (§4 lines 76-80). Branch type compatibility, checked arithmetic, and period-combination rejection are described as compile-time guarantees. The runtime review shows a Money rule returning text and a Decimal overflow panicking. The admission component cannot compensate by inspecting outputs, and the proposal correctly says it should not. Revision: either land a scalar type pass and checked arithmetic upstream and pin the engine revision containing them, or have the bundle manifest record those invariants as unverified. Do not list them as supported semantics on the basis of a green compile.

**4. Blocker for acceptance tests. Evidence independence has no denominator rule** (§6 lines 104-106). The workload arithmetic, expected equals compared plus excluded plus failed, is right. But "borrowed intermediates do not count as independent validation" has no corresponding accounting. Also, "exclusions selected before reading the candidate's results" is a wall-clock ordering that admission cannot verify. Revision: exclusions live inside the frozen workload manifest. Admission checks that the workload hash is on an allowlist signed by the baseline owners' key, not by the generation pipeline. Report coverage per claim, with borrowed inputs as a third bucket that lowers independent coverage.

**5. Contradiction. Scenario hashes can leak household data** (§7 line 114 versus line 118). If a scenario includes household-value overrides, its public canonical hash is a guessable digest of personal attributes, which line 118 forbids. Revision: only rule-level pins may appear in a publicly hashed scenario. Household-specific overrides use a keyed digest and a privately stored scenario object with a random identifier.

**6. Follow-up. Activation and garbage collection race across two stores** (§3 line 68, §8 line 132). "Verified object availability in one transaction" is a Postgres transaction reading an object store, and GC on the object store can delete between check and commit. Revision: track references in Postgres and drive GC from a Postgres query, never from object-store listing. Deferred until Postgres enters the build.

**7. Follow-up. Two sequences compete** (§9 table versus §10 lines 152-154). §9 describes a five-stage from-scratch milestone including generated clients, WASM, and a gateway. §10 says to repair the observed losses in existing repositories first. The document does not say which starts. My recommendation below resolves this: build the slice in a new workspace, and treat upstream engine fixes as its first dependency work.

**8. Overengineering for milestone one.** The five-module monorepo with checked-in generated bindings (§2), rule lineage edges (§5 line 88), key rotation and revocation advisories (§6 line 108), on-demand composition (§7 line 114), the bill state machine (§8 lines 126-130), and a gateway plus worker pool plus Postgres plus object store plus isolated signer (§8 line 122). None is needed to prove the identity contract. The existing signed corpus release already provides source snapshots and Ed25519 provenance, so §3's source-snapshot object should consume it rather than replace it.

**9. Missing acceptance tests** to add to §9 line 146:

- Python client rejects a request with an unknown field and returns trace fields byte-equal to the CLI.
- Evidence referencing a workload hash not on the baseline allowlist fails admission.
- Adding a second admission record leaves the build hash unchanged, which tests the non-circularity claim at line 66.
- A bundle compiled by one engine build and loaded by another with the same artifact format is rejected or recorded as a different execution identity.
- Duplicate case identifiers in oracle output fail the comparison.
- A legal date outside supported versions returns the unsupported-history error rather than the nearest version.
- Rollback restores the predecessor pointer and reproduces the earlier result with its original identity.

**Preferences.** Use a filesystem content-addressed store and atomic-rename compare-and-swap for the publication pointer in the first build. Postgres arrives with durable jobs in a later stage. Keep TypeScript out entirely until stage four.

## Readiness and recommended initial scope

The proposal is ready to begin a bounded build after three decisions are written down: the execution identity struct, the counterfactual rule that pins are runtime scenarios and never mutate the artifact hash, and whether the type checker and checked arithmetic land upstream before or during the slice. It is not ready to begin a monorepo migration or service replacement.

Recommended scope, a new local `axiom-core` Cargo workspace:

- **Engine dependency.** Pin `axiom-rules-engine` as a git dependency at the reviewed revision or a fork revision containing the two engine fixes and `deny_unknown_fields` on requests. No policy logic is reimplemented. Compilation and execution call the crate.
- **Contract crate.** Bundle manifest, workload manifest, evidence, admission, publication, and execution identity types. Canonical encoding, emitted JSON schemas with additional properties denied, and fixtures.
- **Bundle builder.** Takes a pinned RuleSpec checkout and an existing signed corpus release reference, compiles through the engine, writes content-addressed objects to a local directory.
- **Strict executor.** Takes bundle hash, expected engine identity, request, and scenario. Verifies resident bytes, rejects assessment dates, runs the engine, returns the response plus the execution identity actually used.
- **Deterministic admitter.** Verifies an evidence record against a frozen workload manifest with full denominator accounting and signs with a file-loaded key in a separate process. No network.
- **Python client.** Generated from the emitted schemas, tested for byte equality against the CLI on the shared fixtures. This replaces the drifting hand-written models.
- **Program.** One existing small program with two source revisions and an effective-date change, selected from a country repository with an independently authored workload.

Exit condition: both revisions reproduce offline, changed bytes fail verification, a pinned scenario yields a different identity and an empty scenario yields the baseline identity, an incomplete or duplicated comparison fails admission, and a rollback reproduces the earlier result.

## Residual uncertainties

Which program has two clean source revisions and an independent workload is unknown. Whether upstream will accept the engine changes, and on what timeline, determines whether the slice pins a fork. The engine's descriptive period semantics may make the annual-versus-monthly guarantee unattainable without a language change. Nothing here is formal agreement, and no code is approved or merged.
