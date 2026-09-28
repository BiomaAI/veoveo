# Gateway Audit Persistence

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| SurrealDB 3.2.4 | Store-owned `audit_event` records and atomic outbox writes; gateway adapters encode the typed payload in `details.event` |
| JSON and Serde | Policy-event persistence v1 is the deployed envelope without `policy_event_format`. New writes set that field to `veoveo.ai/gateway-policy-event/v2`. Authentication and tool-outcome payloads keep their existing formats. |
| MCP 2026-07-28 and RFC 6570 | Template discovery and resource argument completion use `ResourceTemplateUri` under the [foundation profile](../../../../types/DESIGN.md#resource-templates); concrete reads use resource targets. Literal-only templates are valid. |
| Veoveo policy contract | `AuditEvent` records the action and `PolicyDecision` without reevaluating historical authorization. Shared types live in [MCP contract](../../../../../mcp/contract/DESIGN.md). |

## Encoding And Admission

`canonical_policy_record` validates agreement between the event's action/target and
its decision before Store writes. A `resource_template` target is valid for
`resources_templates_list` or `completion_complete`. A concrete resource, Artifact,
or usage target cannot stand in for a declaration under those actions. Prompt
completion keeps its prompt target. Version 2 writes the current typed event and an
explicit format marker. Event identity, attribution, outcome and timestamps keep
their existing representation.

## Version 1 Reader

`policy_codec/v1.rs` owns read-only DTOs for the old envelope. It decodes URI fields
as text, then examines the recorded action before constructing a current target.
Completion's resource target becomes a resource-template target. Template discovery
also converts its historical Artifact and usage target variants. Literal-only
declarations follow the action in the same way. Concrete reads and prompt completion
preserve their target kinds. The adapter applies the conversion to both copies of
the target and checks that the resulting action and target agree.

Reading does not rewrite a row, reauthorize an old action, or change the recorded
effect, reason, rule, identity or timestamp. Unknown format markers, invalid template
syntax and inconsistent event/decision pairs fail with a diagnostic that names the
unsupported format or correction needed without echoing the payload. Operators
retain those records and inspect them with compatible tooling. Version 1 cannot
contain the version 2 target variant. Live requests use only the current model.

## Upgrade And Recovery

The gateway owns this adapter. It supports the deployed unversioned envelope and
version 2 through the Foundations Phase 4 audit replacement. Retire the v1 reader
only after that replacement is accepted and the installation has completed its
declared old-table retirement under [CE-12](../../../../../docs/CONTRACT_EVOLUTION.md#ce-12-one-audit-record-per-logical-action).

Version 2 is a coordinated installation upgrade. Drain gateway requests and stop all
gateway replicas before replacing them; upgrade clients that consume typed policy
decisions, including the paired Console release. Older readers cannot decode the
new target kind, so overlapping old and new gateway replicas is unsupported. Preserve
a database snapshot and the matching gateway/configuration images before the first
v2 write. A rollback to an older reader requires restoring that snapshot while the
gateway is drained. Preserve later records separately for inspection; do not delete
or downgrade their target kinds to make an old reader accept them. This step performs
no destructive conversion of stored records.

## Qualification

Native cases cover v1 concrete and prompt targets, expressive and literal-only
completion templates, historical usage/Artifact declarations, and v2 round trips.
Negative cases cover unknown markers, wrong-version targets, malformed templates,
action/target disagreement and redacted failures. An isolated pinned Store test uses
separate writer and reader connections, mixes both formats, compares the retained
v1 row after reading, and proves invalid new writes add no record. Installation
drain and restore qualification is tracked in the foundations plan.
