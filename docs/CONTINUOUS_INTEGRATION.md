# Continuous Integration

The current workflow records tests run on the development host and presents their
result on GitHub. GitHub does not build Veoveo, operate Kubernetes, or run GPU
acceptance. This is a temporary arrangement while a larger qualified CI environment
is assembled.

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| GitHub Actions workflow syntax | informational presentation on `push` and `pull_request`; no required check or delivery gate |
| SHA-256 | build-input content identity for the committed local report |
| `veoveo.io/local-test-report/v2` | repository-owned JSON profile for locally executed command results |
| OCI Image Spec | immutable image identities recorded by existing build and deployment acceptance commands |
| Kubernetes API | target-cluster deployment and recovery acceptance executed only from the development host |
| NVIDIA CUDA, Vulkan, RTX, and NVENC | mandatory hardware execution for simulation, rendering, perception, and video acceptance |
| Chrome DevTools Protocol | headed browser verification with hardware-backed WebGPU or WebGL |

## Temporary Local Workflow

Run each relevant existing command through the report wrapper. The wrapper executes
the command directly without a shell, streams its normal output to the terminal, and
updates `testing/local-test-report.json` whether the command passes or fails.
It supplies the repository-managed `protoc` binary unless the caller explicitly
sets `PROTOC`, which keeps transitive Prost build scripts independent of host packages.

```bash
cargo xtask test-report run --name rust-workspace -- \
  cargo xtask enforce rust
cargo xtask test-report run --name python-sdk -- \
  cargo xtask enforce python
cargo xtask test-report run --name console-tests -- \
  npm --prefix apps/console/web test
cargo xtask test-report run --name console-lint -- \
  npm --prefix apps/console/web run lint
cargo xtask test-report run --name console-build -- \
  npm --prefix apps/console/web run build
```

Use the same wrapper for focused service, image, Kubernetes, GPU, and browser
acceptance. A deployed check keeps its existing typed arguments:

```bash
cargo xtask test-report run --name bioma-platform -- \
  cargo xtask smoke bioma-verify
cargo xtask test-report run --name uav-browser -- \
  cargo xtask smoke uav-showcase-browser-verify \
    --public-base-url https://veoveo.bioma.ai \
    --chrome-cdp-url http://127.0.0.1:9222
```

Check names are stable lowercase identifiers. Reusing a name replaces its previous
result. A build-input change makes the earlier report stale; the next recorded command
starts a report for the new build. The report contains command names, completion times,
durations, and concise results. Full terminal logs remain local.

The build identity excludes `docs/**`, root Markdown files, and
`tools/screenshots/**`. Those paths describe the repository or produce documentation
media; they do not change the Veoveo product build. Component-local contract documents
remain build inputs because hosted servers can compile them into their well-known MCP
resources. Product code, package and image definitions, lockfiles, deployment material,
workflows, and other repository tooling also remain build inputs.

Display the current state before committing it:

```bash
cargo xtask test-report show
git add testing/local-test-report.json
```

The GitHub workflow runs only `cargo xtask test-report show --github-summary`. A
matching report containing only successful commands appears green. A failed command,
missing report, or stale source appears red with a summary of the recorded checks.
Neither result blocks a commit, push, deployment, or direct work on `main`; the
repository has no required-check rule.

The report is an engineering status note. It is not release provenance, an audit
attestation, or a security boundary. Its purpose is to keep known local build failures
visible while Veoveo relies on one qualified development host. Documentation validation
remains a separate local responsibility and does not turn the Build status red.

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
