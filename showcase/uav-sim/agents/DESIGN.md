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

## Installation And Cutover

The [UAV chart](../deploy/helm/DESIGN.md) installs an immutable ConfigMap with `manifest.json` and `0001_mission_state.sql` as
data keys. Calculate its revision with `runtime_config_revision` and place that
exact name and digest in the approved runtime template. Bioma's installation values
bind its kernel image, model connection, namespace and resource ceilings.

Managed provisioning must pass before transferring existing pilots. Drain their
old workloads, prove their exact runtime and OAuth identities, retain their physical
volumes and transfer the existing signing keys. Never attach a fresh volume to an
adopted pilot or run the old and managed workers together. The migration removes
per-pilot Helm ownership after the transfer.

Bioma's explicit `pilot_recovery` Rust test verifies the four drained memory
archives against their SHA-256 manifest, restores them into separate temporary node
directories and compares contents and filesystem metadata with the original
archives. `VEOVEO_PILOT_EXPORT_DIRECTORY` selects the private export directory and
`VEOVEO_PILOT_EXPORT_NODE` selects the Docker node. It removes its temporary
directories without mounting or writing a live claim. Record restoration is a
separate prerequisite to the registry adoption transaction.
The check uses the existing `tar` parser at exactly 0.4.46, verified against its
upstream release on September 19, 2026. Entry contents, links, ownership, mode and
modification time are compared independently of filesystem enumeration order.

## Shared Pilot Definition

Bioma publishes one `uav-pilot` definition for four managed instances. Instance IDs,
OAuth principals, signing keys and memory volumes remain distinct. A pilot requires
one active control grant for its session and uses the returned vehicle and mobility
profile. Vehicle assignment never appears in definition parameters or instructions.

The shared revision subscribes to `uav-sim://control-grants` and
`uav-sim://mission-plans`. These resources publish domain changes. Task completion
also wakes its owning instance. Vehicle resources have no update publisher and are
read on demand; telemetry does not cause periodic model calls.

The installation-only consolidation procedure lives in
[`examples/bioma/acceptance`](../../../examples/bioma/acceptance/DESIGN.md).
