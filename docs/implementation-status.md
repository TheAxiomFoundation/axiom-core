# Initial build checkpoint

4 September 2026. Implemented in a new local repository on
`build/bundle-execution`. Existing Axiom repositories remain unchanged.

The [proposal](architecture.md) incorporates the decisions from Fable's Subfleet
review of the [frozen first draft](architecture-v1.md). The [review](fable-review.md)
and [metadata](fable-review-metadata.json) preserve the exact model, run and
reviewed content. Fable reviewed the proposal, not this implementation.

## Delivered

The real pinned Axiom engine compiles an explicit source closure into an unsigned
development bundle. Verification binds source bytes, executable bytes, compiler
options, dependency lock and executing binary to an expected digest, and
recompiles the stored sources. Strict native request handling preserves pins and
complete traces. A thin Python transport produces the same receipt as the CLI.

The [independent implementation review](implementation-review.md) found one
actionable defect: the builder could emit a file above the reader's size limit.
That defect was fixed before file creation and independently rechecked. The
review has no unresolved actionable findings within the stated prototype scope.

## Verification

All checks passed on the final Rust implementation and Python transport:

| Check | Result |
|---|---|
| `cargo fmt --all --check` | Passed |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | Passed |
| `cargo test --locked --workspace` | 21 tests passed |
| `cargo build --locked --workspace` | Passed |
| `python3 -m unittest discover -s python/tests -v` | 11 tests passed |
| `python3 scripts/demo.py` | Actual engine outputs 162.96, 212.96 and 0 |

The two annual values come from synthetic versioned rules. The final zero comes
from a native rule pin. These are software conformance fixtures, not legal or
oracle validation. The demo saves a copy of the exact CLI binary alongside its
bundle, original requests and receipts so a later CLI rebuild does not remove
the executable needed for replay.

The completed local demo is in `output/synthetic-5s_xwosw/` (ignored by Git), with
bundle digest
`fcb5a5ee7f95504790e11ec5742b5fa74bf5ad943a69353f4b003f26c05463cd`.
Run the demo again to make a new set without overwriting existing files.

## Remaining target architecture

The first production-oriented milestone is still ahead: one real program over
two signed source revisions, with an independently authorized expected workload.
Signed admission, publication, source-authority integration, generated rich
clients, services and durable jobs are not implemented here. The pinned engine's
unresolved type and arithmetic guarantees also remain upstream work.

This repository is local. No GitHub repository, PR, deployment, publication, or
migration of existing Axiom repositories was performed.
