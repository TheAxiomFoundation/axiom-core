# Development bundle and execution contract

This document describes the implemented local checkpoint. The broader
[architecture](architecture.md) is a proposed destination.

## Source and executable identity

`axiom/build-spec/v0` contains a root target and a map of module identifiers to
exact UTF-8 RuleSpec strings. Imports resolve only within that map. Missing
imports and modules outside the resolved closure are errors. A build never
searches a checkout or fetches sources from a network.

`axiom/development-bundle/v0` stores those source strings, a manifest, its SHA-256,
and the compiled artifact as a JSON string. The artifact's string bytes are
hashed exactly, including whitespace. Source comments affect source identity
even when executable bytes do not change.

The manifest binds the root, each source hash, source-closure hash, artifact
hash, strict compiler options, and engine identity. Engine identity contains the
pinned upstream revision, native engine version and artifact format, the
dependency lockfile hash, and the SHA-256 of the executing host binary. In this
checkpoint the host is the CLI, or the embedding test/application binary when
using the Rust library. Identifying that binary fails closed if it cannot be read.

Structured identities use compact UTF-8 JSON with recursively sorted object
keys. Array order remains meaningful, except pins are explicitly sorted by rule
name. This is a versioned local serialization convention, not an implementation
of a general JSON canonicalization standard. Native scalar serialization remains
the authority; semantically similar but differently represented requests may
have different context identities.

The caller supplies an expected manifest digest independently of the mutable
bundle. The verifier checks it, all source and artifact bytes, the running engine
identity, and the real engine's validated artifact loader. It also recompiles the
stored sources and requires the canonical executable bytes to match exactly.
There is no unverified constructor for the facade's `VerifiedBundle` handle.

Rebuild verification costs a compilation on each CLI execution. That deliberate
local tradeoff keeps the initial checkpoint small. A future verified cache or
admission record needs its own immutable identity and invalidation rules.

## Execution boundary

`run` consumes the existing native `CompiledExecutionRequest`. The Rust engine's
types own semantics; the facade adds rejection checks where Serde would otherwise
silently ignore fields, including buffered tagged scalar values and flattened
periods. The native response is retained whole under `result`.

Requests are parsed from their original bytes so duplicate object keys cannot
disappear in a map conversion. Unknown fields, duplicate pins, reversed input,
relation or query intervals, and unsupported assessment dates are errors. Any
`assessment_date` key is rejected, including null, because knowledge-time
selection is not implemented. Valid-time rule versions use the native engine's
interval semantics. This does not infer dimensional validity of arbitrary periods.

The pinned engine selects derived-rule and parameter versions using
`period.start`. It does not split or prorate a query across version boundaries,
and a `month` label does not require a calendar-month interval. A custom period
spanning December 2026 through January 2027 therefore selects the 2026 version
when an input covers that whole interval. The facade checks interval ordering;
it preserves these native temporal semantics.

Dataset binding uses the native strict binder before execution. Rule pins use
the existing engine's rule names. A pin creates a request scenario and modifies
an in-memory execution copy; it does not rewrite the stored baseline artifact.

An input record's `entity` field participates in relation-slot diagnostics, but
scalar lookup uses the input name, `entity_id`, and period. Changing only `entity`
can therefore leave execution unchanged when no relation diagnostic applies.
Strict parsing rejects unrecognized fields; it does not give recognized fields
semantics beyond those implemented by the pinned engine.

The private `axiom/execution-receipt/v0` records:

- The bundle and executable hashes, running engine identity, and wire version.
- The explicit scenario (sorted native pins), including a hash for the empty
  scenario, normalized native queries, and execution mode.
- A context hash over that object. The complete native result and trace are
  returned separately under `result`.

The context does **not** hash the household dataset or the result. Two households
with the same query identifiers, periods, outputs and scenario can share a
context hash. Preserve the original private request alongside a receipt for
replay. No result signature or public receipt authenticity is claimed.

Python is a subprocess transport. It sends the unchanged JSON object on stdin,
retains the entire Rust response, and propagates structured errors. Python does
not have a second evaluator or mirrored typed request/response models. Its
timeout bounds one subprocess; it is not a durable job or engine resource quota.

## Trust and unsupported capabilities

Every bundle and receipt declares `development_unsigned`. An expected digest
can pin self-authored source but cannot make it an official release. These
fixtures do not establish legal interpretation or independent oracle coverage.

Source strings must be trusted development input. The wrapper does not repair
the engine's unresolved type and arithmetic guarantees, catch every possible
panic, or provide hostile-program sandboxing. The CLI limits each input to
16 MiB; that is not an execution memory or CPU limit.

Exact host-binary matching rejects bundles from a different build, even if its
engine version string matches. The binary hash is measured locally from the
current executable path and is not an attestation against a hostile host or
filesystem. It does not describe dynamically loaded system-library state.

Bundle files use exclusive creation and are never overwritten. The builder
rejects serialized bundles over the same 16 MiB reader limit before opening
the output file. A failed write
may leave a partial file that cannot verify. This is not a transactional object
store or compare-and-swap publication mechanism.

Signed admission, source-authority integration, knowledge-time reconstruction,
public or tenant-scoped scenario identity, publication, garbage collection,
HTTP/WASM adapters, and durable jobs remain future deliverables.
