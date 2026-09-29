# UAV Showcase Installation

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Helm | Application chart with the repository `veoveo-common` library API; JSON Schema draft 7 validates values |
| Kubernetes | Deployments, Services, networking policies and persistent claims for the simulator and recording forwarder; core/v1 immutable ConfigMap for the reviewed agent template |
| Veoveo managed runtime | Kernel JSON manifest and DuckDB SQL memory migration; sorted-map SHA-256 binds exact ConfigMap data to an installation-approved runtime template |
| Installation world | Strict UAV installation binding JSON; SHA-256 Pod annotations on runtime and companion; runtime admission checks the selected binding before accepting configuration |
| Container delivery | Exact OCI runtime image digests in production; NVIDIA GPU allocation for the simulator |

## Ownership

The chart owns the GPU simulator and independent UAV MCP companion. Its retained
claims hold runtime caches and recording-forwarder data. Installation values supply
network placement and credential references.

`agentTemplate.enabled` installs the immutable pilot ConfigMap into
`agentTemplate.namespace`. Its name contains the first twelve characters of the
canonical data digest. The platform's approved runtime template must select this
exact name and full digest. `files/agent-template/` is the canonical packaged manifest
and memory schema; seed instructions belong under `showcase/uav-sim/agents/`.

Individual pilots are managed instances. Their definitions, workload identities,
OAuth registrations, signing keys and memory claims do not belong to this chart.
The platform's managed namespace admission and network policy govern those workloads.
The UAV MCP companion discovers eligible App message targets from the managed registry
and current domain grants.

## World Deployment

`world.bootstrap` selects an installation-owned ConfigMap, key and content SHA-256.
Both Deployments mount the document read-only and receive `UAV_SIM_WORLD_BOOTSTRAP_FILE`.
The content digest changes both Pod templates. The GPU Deployment uses `Recreate`
because a process admits one immutable world. Cache and recording claims keep their
existing identities.

The runtime pins the expected binding before accepting adapter traffic. A companion
with a different document cannot win first admission during overlapping rollouts.
Deploying a changed world requires drained physical work. An upgrade from a runtime
without this admission check requires draining both workloads before selecting the
chart and runtime together. A rejected mismatch leaves the runtime unconfigured; the
matching companion supplies the normal admission request.

After a database reset, the installation publishes its scenario through Frames with
`uav-world-publish`, then commits the resulting file and digest together. Installed
acceptance resolves that publication through Frames before starting flight work.

## Cutover

Bioma adopted its four retained pilots before removing their chart resources. Their
old workloads remain drained while the suspended release is updated to this chart.
The managed claims bind the same retained physical volumes in the managed namespace.
Removing the superseded chart resources must not delete those claims, signing keys or
physical memory. Resume GitOps only against the chart that has no per-pilot resources.
