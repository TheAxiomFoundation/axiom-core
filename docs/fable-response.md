# Response to Fable and initial build scope

Fable reviewed proposal v1 (`9c308dd1c32cbf04270a5d2f84e7e86a8fdb63810ac7859cc77967d53e6d4842`) through Subfleet run `20260904-175824-axiom-architecture-proposal`. Its [verbatim review](fable-review.md) found concrete decisions to resolve before implementation. The following are the main author's decisions, not claims of subsequent Fable approval.

| Finding | Decision |
|---|---|
| Undefined execution identity | Define a canonical execution-context object and its hash. It binds bundle, executable, engine build, wire version, explicit scenario, and normalized queries. It identifies execution context, not a household or uniquely identified result. |
| Assessment date accepted without semantics | Reject any supplied `assessment_date`, including null, in the initial strict boundary. Capability discovery declares knowledge-time selection unsupported. |
| Compiler guarantees exceed existing engine | The initial bundle is explicitly experimental and unsigned. It promises byte/engine identity and lossless request/response handling; it does not claim a proven type system, checked arithmetic, or period-unit inference. Those require actual engine changes and remain separate milestones. |
| Independence and exclusions not enforceable | Production admission requires a workload manifest, including exclusions and independence categories, authorized by baseline owners. Generic comparison arithmetic alone does not qualify a workload. This admission subsystem is deferred from the first prototype. |
| Scenario digests can leak household values | Public scenarios are expressly public policy changes. Private scenarios require tenant-scoped keyed identity/opaque handles in a future service. Initial execution receipts are local/private and are not published; no household-input hash is put in the public bundle. |
| Activation/GC cross-store race | Later storage needs transactional reference reservations and fenced GC. A file rename alone is not compare-and-swap; any local publication pointer needs a lock and predecessor check. Both activation and GC are deferred from the initial checkpoint. |
| Competing sequences / too much infrastructure | Start a new local `axiom-core` Cargo workspace with a pinned real engine dependency. Build identity/execution first; then independently authorized evidence/admission, real released-program integration, and finally services/durable jobs. Current repositories remain reusable dependencies and targets for engine fixes. |

## Initial checkpoint to implement now

1. A local Cargo workspace depending on the real `axiom-rules-engine` at exact revision `d142c645917817cf590e036fb99f99b2d4780e1a`.
2. A bundle builder resolving an explicit in-memory RuleSpec module closure, retaining source bytes and the compiler-produced executable, with separate source, artifact, and manifest hashes. The bundle is labeled unsigned/development; it makes no official source or legal validation claim.
3. Verification against a caller-supplied expected bundle digest and the running engine identity, with offline source/artifact integrity checks. Hashes supplied inside a self-authored bundle alone do not establish trust.
4. A strict execution boundary that reuses native engine request/response types, preserves pins and traces, rejects unknown/dropped fields, duplicate JSON keys, invalid intervals, and unsupported assessment dates, and returns the actual execution-context identity. Pins are request scenarios; they never change the stored baseline artifact hash.
5. A thin Python transport that passes JSON to the Rust CLI without maintaining another model schema. Generated rich Python models remain a later deliverable; the Rust parser is the authority in this checkpoint.
6. Conformance tests on clearly labeled synthetic RuleSpec fixtures, executed by the real engine: imports, historical versions, baseline versus empty/pinned scenario, unknown fields at nested enum/period locations, trace preservation, tampering, wrong engine identity, and offline replay. These fixtures prove software behavior, not legal coverage or independent policy validation.

The checkpoint intentionally precedes Fable's larger first release milestone. Selecting an existing program with two signed source revisions and an independently authorized workload is required before claiming that larger milestone complete. There is no admission signer, production activation, object-store service, database, gateway, or rewrite of calculation semantics in this initial checkpoint.
