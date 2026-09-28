# Gateway Audit Persistence

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| SurrealDB 3.2.4 | Store-owned `audit_event` records and atomic outbox writes; gateway adapters encode the typed payload in `details.event` |
| JSON and Serde | Policy events require `policy_event_format: veoveo.ai/gateway-policy-event/v2`. Authentication and tool-outcome payloads use their own current formats. |
| MCP 2026-07-28 and RFC 6570 | Template discovery and resource argument completion use `ResourceTemplateUri` under the [foundation profile](../../../../types/DESIGN.md#resource-templates); concrete reads use resource targets. Literal-only templates are valid. |
| Veoveo policy contract | `AuditEvent` records the action and `PolicyDecision`. Shared types live in [MCP contract](../../../../../mcp/contract/DESIGN.md). |

## Encoding And Admission

`canonical_policy_record` validates agreement between the event's action/target and
its decision before Store writes. A `resource_template` target is valid for
`resources_templates_list` or `completion_complete`. A concrete resource, Artifact,
or usage target cannot stand in for a declaration under those actions. Prompt
completion keeps its prompt target. The writer stores the typed event and its format
marker. The reader requires that current format and applies the same validation.

Missing or unknown markers, invalid template syntax and inconsistent event/decision
pairs fail with redacted diagnostics. The reader neither converts obsolete shapes nor
changes a recorded policy decision. Contract changes ship with coordinated consumers
and a fresh reference installation under the Foundations hard-cut rule.

## Qualification

Native cases cover current concrete, prompt and template targets, event identity,
marker admission and inconsistent actions or targets. An isolated pinned Store test
uses separate writer and reader connections and proves invalid writes add no record.
Installed current-format qualification belongs to the foundations plan.
