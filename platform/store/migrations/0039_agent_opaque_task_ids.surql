-- MCP 2026-07-28 task identifiers are opaque gateway handles. The hard cut
-- intentionally discards unfinished draft-protocol deliveries.
DELETE agent_task;

REMOVE INDEX IF EXISTS agent_task_unique ON TABLE agent_task;
REMOVE INDEX IF EXISTS agent_task_retention_pin ON TABLE agent_task;
REMOVE FIELD IF EXISTS task ON TABLE agent_task;
DEFINE FIELD IF NOT EXISTS task_id ON TABLE agent_task TYPE string ASSERT string::len($value) > 0 AND string::len($value) <= 128;
DEFINE INDEX IF NOT EXISTS agent_task_unique ON TABLE agent_task FIELDS agent, task_id UNIQUE;
DEFINE INDEX IF NOT EXISTS agent_task_retention_pin ON TABLE agent_task FIELDS task_id, retention_pin_active;
