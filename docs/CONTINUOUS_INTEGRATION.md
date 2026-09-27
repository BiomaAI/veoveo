# Continuous Integration

The repository has no CI. Developers run the checks a change touches on their own
host before committing it, using each framework's native command: `cargo test`,
`cargo clippy`, `npm run build`, the Python test runners, `cargo xtask enforce`, and
`cargo xtask smoke` for end-to-end scenarios. Commits carry no recorded test results.

## Standards And Protocols

| Standard or protocol | Use |
|---|---|
| Git | the source revision each check runs against |
| Cargo, npm, and uv | native test and build commands for Rust, TypeScript, and Python |
| Kubernetes and Helm | disposable clusters for `cargo xtask smoke` scenarios |
| NVIDIA CUDA, Vulkan, RTX, and NVENC | GPU workloads and visual acceptance, which run only on hardware |

## Direction: GPU CI Workers

Continuous integration will run on dedicated ephemeral workers owned by the
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
