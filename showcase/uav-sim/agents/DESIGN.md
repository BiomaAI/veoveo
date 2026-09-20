# Governed UAV Pilot Template

## Standards And Protocols

The template uses the generic Veoveo kernel JSON manifest, DuckDB SQL memory
migrations and the managed-agent revision contract. Kubernetes core/v1 immutable
ConfigMap data is bound by the shared sorted-map SHA-256 profile. Model and MCP
connections use the kernel's current approved connection and MCP 2026-07-28
contracts. These files are installation inputs, not a public UAV protocol.

## Ownership

`template/manifest.json` declares trusted memory, context and gateway wiring.
Closed session and vehicle parameters describe the requested assignment. Current
UAV control grants determine actual authority. Published definition revisions own
instructions, tools, subscriptions and episode budgets. The manager supplies each
instance's identity, credentials and retained volume.

`template/instructions.md` is seed content for explicit definition creation. It
does not reconcile over edits made through the API or Console. Installation replaces
its named pilot/session placeholders before publication. Authored content receives
no environment interpolation in the kernel.

## Installation And Cutover

Install an immutable ConfigMap with `manifest.json` and `0001_mission_state.sql` as
data keys. Calculate its revision with `runtime_config_revision` and place that
exact name and digest in the approved runtime template. Bioma's installation values
bind its kernel image, model connection, namespace and resource ceilings.

Managed provisioning must pass before transferring existing pilots. Drain their
old workloads, prove their exact runtime and OAuth identities, retain their physical
volumes and transfer the existing signing keys. Never attach a fresh volume to an
adopted pilot or run the old and managed workers together. The migration removes
per-pilot Helm ownership after the transfer.
