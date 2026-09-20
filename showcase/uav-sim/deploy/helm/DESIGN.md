# UAV Showcase Installation

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Helm | Application chart with the repository `veoveo-extension` library API; JSON Schema draft 7 validates values |
| Kubernetes | Deployments, Services, networking policies and persistent claims for the simulator and recording forwarder; core/v1 immutable ConfigMap for the reviewed agent template |
| Veoveo managed runtime | Kernel JSON manifest and DuckDB SQL memory migration; sorted-map SHA-256 binds exact ConfigMap data to an installation-approved runtime template |
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

## Cutover

Bioma adopted its four retained pilots before removing their chart resources. Their
old workloads remain drained while the suspended release is updated to this chart.
The managed claims bind the same retained physical volumes in the managed namespace.
Removing the superseded chart resources must not delete those claims, signing keys or
physical memory. Resume GitOps only against the chart that has no per-pilot resources.
