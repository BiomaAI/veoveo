# Governed UAV Pilot Template

## Standards And Protocols

The template uses the generic Veoveo kernel JSON manifest, DuckDB SQL memory
migrations and the managed-agent revision contract. Kubernetes core/v1 immutable
ConfigMap data is bound by the shared sorted-map SHA-256 profile. Model and MCP
connections use the kernel's current approved connection and MCP 2026-07-28
contracts. These files are installation inputs, not a public UAV protocol.

## Ownership

[`manifest.json`](../deploy/helm/files/agent-template/manifest.json) declares trusted memory, context and gateway wiring.
The session parameter selects the simulation. Each instance discovers its vehicle
through its authenticated principal’s current UAV control grant. Published definition revisions own
instructions, tools, subscriptions and episode budgets. The manager supplies each
instance's identity, credentials and retained volume.

`instructions.md` is seed content for explicit definition creation. It
does not reconcile over edits made through the API or Console. The session comes from the reviewed runtime context. Authored content
receives no environment interpolation in the kernel.

## Installation

The [UAV chart](../deploy/helm/DESIGN.md) installs an immutable ConfigMap with `manifest.json` and `0001_mission_state.sql` as
data keys. Calculate its revision with `runtime_config_revision` and place that
exact name and digest in the approved runtime template. Bioma's installation values
bind its kernel image, model connection, namespace and resource ceilings.

The manager provisions each pilot's workload, identity, signing key and memory
claim. Resuming an existing instance preserves its retained identity and storage.
The lifecycle controller drains its writer before replacing a running generation;
missing retained storage requires recovery instead of a fresh volume.

## Shared Pilot Definition

Bioma publishes one `uav-pilot` definition for four managed instances. Instance IDs,
OAuth principals, signing keys and memory volumes remain distinct. A pilot requires
one active control grant for its session and uses the returned vehicle and mobility
profile. Vehicle assignment never appears in definition parameters or instructions.

The shared revision subscribes to `uav-sim://control-grants` and
`uav-sim://mission-plans`. Task completion also wakes its owning instance. The
pilot receives command acknowledgements directly in tool results. The server also
publishes those acknowledgements on vehicle resource URIs through `apply_command`;
the shared definition omits those subscriptions to avoid duplicate result wakes.
A vehicle change made by another controller becomes visible on the pilot’s next
domain read. Continuous vehicle telemetry does not drive model episodes. Grant
changes and mission-plan events keep their existing subscriptions.

Installation composition and native record-restoration checks live in
[`examples/bioma/acceptance`](../../../examples/bioma/acceptance/DESIGN.md).
