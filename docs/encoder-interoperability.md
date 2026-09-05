# Encoder interoperability

The private core repository owns CI for the encoder-to-core boundary. Its
`encoder-interop` job checks out an immutable commit of the public
`TheAxiomFoundation/axiom-encode` repository, installs its locked Python 3.13
development dependencies with `uv --locked`, and builds the actual core CLI from
the core revision under test. The existing `conformance` job continues to check
core's Rust contracts, Python transport, and native-engine demo.

The encoder's integration suite requires an explicit `AXIOM_CORE_BIN` file path.
Public encoder CI leaves it unset and reports these tests as skipped. It does
not need access to the private core repository. Core CI sets it to the binary
it just built, so the integration suite runs against real software without a
mock, substitute evaluator, or private-repository credential in public CI. An
invalid configured binary fails the tests rather than falling back or skipping.
Core CI requires an executable binary before pytest and checks its JUnit report
for at least one test and zero skips. Test failures, missing or invalid reports,
and an empty or skipped suite all fail the job. Locked dependency installation
also rejects a lockfile that is stale relative to the encoder project metadata.

## What the job verifies

The synthetic fixtures exercise the actual encoder CLI's
`export-core-build-spec` command and core's `build`, `verify`, and `run` commands:

- Explicit two-module closure with a fragment import and native explanation
  fields, including parameter reads and rounding details.
- Exact UTF-8 source retention, including CRLF line endings and Unicode, through
  export and the stored bundle's source hashes.
- Missing dependencies and supplied modules outside the resolved closure are
  rejected by the core compiler boundary.
- Changing a dependency changes bundle and artifact identities and the native
  output. A native zero pin changes execution while retaining the stored bundle
  and its baseline behavior.

Export remains an `unvalidated_candidate`; successful execution creates an
unsigned development receipt. These checks establish software interoperability,
not legal validation or signed admission. Python transports requests and checks
native results; it does not compute policy outcomes.

## Updating the encoder pin

The only encoder revision pin is `ENCODER_REVISION` in
[the CI workflow](../.github/workflows/ci.yml). Use a reviewed, full 40-character
commit SHA that is available from the public encoder repository. A branch name,
tag, or placeholder fails the job before checkout. The job also compares the
checked-out commit to the requested SHA.

To update it, review the encoder changes since the current pin, including its
exporter, integration tests, and dependency lockfile. Change the SHA in a core
PR and run both CI jobs. A passing core PR tests that exact encoder revision
against the core revision being proposed. Advancing the public encoder's main
branch does not silently change this integration dependency.

For a local run, first check out the same encoder commit in a separate directory,
then build core and provide the resulting binary explicitly:

```sh
# From the core checkout:
cargo build --locked --workspace
export AXIOM_CORE_BIN="$(pwd)/target/debug/axiom-core"

# From the separate encoder checkout at ENCODER_REVISION:
env -u UV_FROZEN uv sync --locked --python 3.13 --extra dev
env -u UV_FROZEN uv run --locked --python 3.13 --extra dev python -m pytest \
  tests/test_core_export_integration.py -o addopts='' -q -rs
```

Keep the same binary throughout a run: executable bytes are part of core's
development bundle identity. No existing bundle or compiler implementation is
substituted by this job.
