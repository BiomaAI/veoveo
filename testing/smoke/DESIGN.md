# Installed And Process Smoke Harness

## Standards And Protocols

| Contract | Supported profile |
|---|---|
| MCP `2026-07-28` | Official Rust SDK Discover, Tasks, resource reads and request-scoped subscriptions over Streamable HTTP |
| Kubernetes API | Native `kubectl` reads, rollout watches and Pod-specific port forwards using an explicit installation target |
| Veoveo installation target | Typed origin, Kubernetes coordinates and installed workload selection from `veoveo-deploy-contract` |
| Stream contract | Owner-library run identities, resource builders, completion products and analysis results |
| NVIDIA execution | Installed DeepStream GPU replay under the Stream server's required hardware profile |

## Ownership

`src/bin/smoke.rs` dispatches native process and installed scenarios. Each scenario owns
its domain assertions and cleanup; `cargo xtask smoke` selects build prerequisites and
invokes the harness. Shared support supplies official MCP clients, authentication,
process guards and installation selection. Focused deployment, flight and browser
harnesses own their respective acceptance suites. See the
[code map](../../docs/CODEMAP.md#testing-and-conformance) for those owners.

## Stream Cross-Replica Acceptance

The installed `stream-gpu` smoke accepts `--replica-pods WRITER OBSERVER` to pin
dispatch and observation to different ready Pods of the same ReplicaSet and image.
The operator supplies both Pods; the harness does not scale the Deployment. Each Pod
keeps the installation's required GPU resources. The probe subscribes through the
official SDK to the Task, run and result on the observer while the Task is working.
It requires a completion notification and subsequent run/result invalidations, then
compares completed Task envelopes and typed result reads across both Pods. A fresh
subscription must recover the same completed state, and both subscriptions must cancel.
Pod identities, image IDs and restart counts must stay unchanged throughout the case.

The probe writes `stream-replicas.json` in the requested work directory, including the
created run ID on failure. It dispatches once and waits at most 300 seconds. A Task
that finishes before observer admission fails qualification. Reuse of the report path
is rejected so an uncertain dispatch cannot trigger an automatic replay. Port forwards
and SDK connections belong to the harness and close when it ends. The ordinary replay
checks still require processed GPU frames, detections and Artifact outputs. Live-session
delivery belongs to its GPU owner and requires its separate installed case.

Run against two already-ready Pods with a fresh output directory:

```sh
cargo xtask smoke stream-gpu \
  --installation examples/bioma/installation-target.json \
  --pipeline-id <installed-object-detection-pipeline> \
  --producer-key-secret <installed-recording-producer-secret> \
  --replica-pods <writer-pod> <observer-pod> \
  --work-dir <new-output-directory>
```

The ordinary scenario also requires an environment file containing the installation's
internal assertion signer and Store credentials, plus an installed producer Secret.
Tokens stay out of arguments and receipts. Local port 8797 addresses the writer and
18797 the observer; both must be unused before the run. The sample generator, recording
producer and Artifact checks use the same prerequisites as ordinary `stream-gpu`.
This direct-server check does not qualify public Gateway load balancing or live-session
routing. It complements the installed public MCP and composed-flight cases.
