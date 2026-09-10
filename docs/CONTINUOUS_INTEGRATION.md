# Continuous Integration

The current workflow records tests run on the development host and presents their
result on GitHub. GitHub does not build Veoveo, operate Kubernetes, or run GPU
acceptance. This is a temporary arrangement while a larger qualified CI environment
is assembled.

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| GitHub Actions workflow syntax | informational presentation on `push` and `pull_request`; no required check or delivery gate |
| SHA-256 | per-check materialized input identity and immutable receipt integrity |
| `veoveo.io/test-receipt/v1` and `veoveo.io/local-test-report/v3` | immutable command attempts and their repository-owned index |
| `veoveo.io/test-check-catalog/v1` and `veoveo.io/test-coverage-profile/v1` | owner-reviewed command/input declarations and explicit required coverage |
| OCI Image Spec | immutable image identities recorded by existing build and deployment acceptance commands |
| Kubernetes API | target-cluster deployment and recovery acceptance executed only from the development host |
| NVIDIA CUDA, Vulkan, RTX, and NVENC | mandatory hardware execution for simulation, rendering, perception, and video acceptance |
| Chrome DevTools Protocol | headed browser verification with hardware-backed WebGPU or WebGL |

## Local Evidence Workflow

Run each relevant existing command through the recorder. It executes the command
without an intervening shell and streams output to the terminal. Each attempt writes
one immutable file under `testing/test-receipts/`; `testing/local-test-report.json`
indexes those files by run ID and content digest.

```bash
cargo xtask test-report run --name evidence-receipts -- \
  cargo test -p veoveo-xtask commands::test_report:: -- --nocapture
cargo xtask test-report run --name console-tests -- \
  npm --prefix apps/console/web test
cargo xtask test-report run --name console-lint -- \
  npm --prefix apps/console/web run lint
cargo xtask test-report run --name console-build -- \
  npm --prefix apps/console/web run build
cargo xtask test-report show
```

The exact command and checked-in owner declaration select the input boundary.
`testing/evidence-checks/` initially admits the recorder, image-source helper,
xtask lint/format, Console checks and generated client-type verification. Cargo
closures include transitive local development and build dependencies, workspace
manifests, the complete lockfile and declared external fixtures. Console checks also
include the Rust schema generator and its dependencies. A command name is display
text; changing it cannot hide a failure of the same admitted command.

Other native checks still run through the recorder. Their arbitrary arguments are
not persisted because they can contain credentials. Their source boundary covers
the repository and their environment is explicitly unqualified for reuse. Admission
requires a reviewed descriptor for the exact command; a command-line flag cannot
remove dependencies or elevate an installed test into qualified evidence.

The recorder supplies its managed `protoc` unless `PROTOC` is explicitly set. Admitted
source checks clear `LD_LIBRARY_PATH`, including the parent path Cargo injects into
`cargo run`; child Cargo owns the loader paths for the binaries it builds. Other
commands retain the caller's loader environment. Custom compiler flags, wrappers,
external Cargo configuration and unsupported execution overrides make evidence
unqualified for reuse. Actual Rust, Cargo, formatter, linter, Protoc and relevant
Node/npm versions are recorded. Selected Rustup toolchains are identified by those
observed versions. No secret environment value or public hash of one is recorded.

Input identities use materialized bytes and modes. They survive a commit or move to
an input-equivalent worktree. Git clean filters cannot hide changes in the files a
check reads. New and removed files participate; explicit expected files retain a
missing marker. Symlink targets must remain inside the repository and may not expose
ignored files. Unqualified directory links fail with a diagnostic.

Publication briefly locks the worktree index and preserves concurrent results. Failed
attempts remain history. The latest completed attempt of an admitted command selects
its result; an attempt whose inputs changed during execution cannot replace it.
A complete receipt left unindexed by a crash prevents evidence presentation until
the next publication recovers it. Before/after fingerprints detect input changes
across an ordinary run; the recorder does not execute from an immutable filesystem
snapshot and makes no adversarial concurrent-mutation guarantee.

`show` evaluates each current source boundary independently. Stale rows remain
visible; they do not erase unrelated valid observations. A current failure or the
absence of any current passing observation returns an error. An unqualified row
records an observed pass without claiming environment reuse. Commit the index and
its receipt files with the change:

```bash
git add testing/local-test-report.json testing/test-receipts
```

GitHub runs only `cargo xtask test-report show --github-summary`. It presents this
committed local evidence and does not rerun substantive checks or establish current
host compatibility. The report remains an engineering status record, separate from
release provenance and security attestation. The v2 aggregate is retired; its results
remain in Git history and were not converted into scoped receipts.

## Coverage And Remaining Release Work

The recorder's owning contract is
[`tools/xtask/src/commands/test_report/DESIGN.md`](../tools/xtask/src/commands/test_report/DESIGN.md).
Its `verify --profile PATH` command joins explicitly required check identities and
rejects gaps, changed source, current known failures, environment changes, expired
runtime observations and mismatched bindings. Profiles are repository-owned JSON
with schema `veoveo.io/test-coverage-profile/v1`. Source verification checks the
current host's observed tool versions; displaying an old receipt does not renew it.

```bash
cargo xtask test-report verify --profile testing/coverage/iteration-tools.json
```

This profile requires the recorder, image-source helper, xtask lint and formatting
checks. It establishes that tooling coverage only.

The initial runtime adapter admits source checks. Installed, integration and visual
harnesses still need adapters that bind their actual image/configuration closure,
provider and cluster identities, observation time and required GPU evidence. A
source descriptor cannot supply those evidence classes. The generic composition
checks qualify rejection of missing, expired and mismatched runtime fixtures; those
fixtures do not establish that an installed adapter exists.

The selected release closure, including retained images and their compatibility,
must generate its required profile before release coverage can become complete.
That deployment-lock integration remains CE-06 work. Local source coverage must not
be presented as full Computers release acceptance.

Rust retains service/process harness ownership. Console behavior may use maintained
TypeScript browser tooling, and SDK consumers use their own language. Headless
behavioral evidence remains distinct from headed hardware visual acceptance.

## Future Full GPU CI

The permanent system moves execution to dedicated ephemeral workers owned by the
installation operator. GitHub may receive status, but it does not supply the
simulation hardware or cluster authority.

The worker image carries the pinned repository toolchain and attaches only disposable
build storage. A qualified NVIDIA node exposes CUDA, Vulkan, RTX, and NVENC together;
software rendering never satisfies a visual result. Browser workers run headed Chrome
and must prove hardware WebGPU or WebGL before opening a visual workflow.

The eventual pipeline has distinct execution stages:

| Stage | Work |
|---|---|
| Source | Rust, Python, TypeScript, schema, conformance, and documentation checks |
| Images | reproducible BuildKit graph, immutable runtime digests, SBOM, and provenance |
| GPU runtime | simulation-base probes, cuOpt, perception, RTX rendering, and NVENC |
| Deployment | disposable Kubernetes installation, GPU scheduling, identity, MCP, agents, recordings, and recovery |
| Visual acceptance | headed-browser live and replay workflows with exact cadence and latency gates |
| Stability | rolling restart, reconnect, task continuity, recording continuity, and bounded soak |

Artifacts retain the exact source revision, toolchain, image digests, GPU and driver
identity, deployment lock, test output, and performance measurements. Workers start
clean and surrender cluster credentials after each run. Expensive image and model
caches may persist by immutable digest, while workspaces and runtime state do not.

No part of the future design is a current delivery gate. Required checks, automatic
deployment, and merge policy need a separate decision after the worker pool is stable
and its results are repeatable.
