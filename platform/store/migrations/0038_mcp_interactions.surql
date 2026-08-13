-- Durable multi-round tool requests. Request state is an opaque integrity-
-- protected value at the protocol boundary; the database stores its digest
-- beside the typed outstanding requests and accepted responses.

DEFINE TABLE IF NOT EXISTS mcp_interaction SCHEMAFULL TYPE NORMAL CHANGEFEED 30d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE mcp_interaction TYPE string;
DEFINE FIELD IF NOT EXISTS tenant ON TABLE mcp_interaction TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS owner ON TABLE mcp_interaction TYPE record<principal>;
DEFINE FIELD IF NOT EXISTS work_context ON TABLE mcp_interaction TYPE record<work_context>;
DEFINE FIELD IF NOT EXISTS profile ON TABLE mcp_interaction TYPE record<profile>;
DEFINE FIELD IF NOT EXISTS server ON TABLE mcp_interaction TYPE record<mcp_server>;
DEFINE FIELD IF NOT EXISTS task ON TABLE mcp_interaction TYPE option<record<task>>;
DEFINE FIELD IF NOT EXISTS method ON TABLE mcp_interaction TYPE string ASSERT string::len($value) > 0 AND string::len($value) <= 128;
DEFINE FIELD IF NOT EXISTS request_state_digest ON TABLE mcp_interaction TYPE string ASSERT string::len($value) = 64;
DEFINE FIELD IF NOT EXISTS input_requests ON TABLE mcp_interaction TYPE object FLEXIBLE;
DEFINE FIELD IF NOT EXISTS input_responses ON TABLE mcp_interaction TYPE option<object> FLEXIBLE;
DEFINE FIELD IF NOT EXISTS state ON TABLE mcp_interaction TYPE "input_required" | "resuming" | "completed" | "cancelled" | "expired" DEFAULT "input_required";
DEFINE FIELD IF NOT EXISTS created_at ON TABLE mcp_interaction TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS updated_at ON TABLE mcp_interaction TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS expires_at ON TABLE mcp_interaction TYPE datetime;
DEFINE FIELD IF NOT EXISTS completed_at ON TABLE mcp_interaction TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS revision ON TABLE mcp_interaction TYPE int DEFAULT 0 ASSERT $value >= 0;
DEFINE INDEX IF NOT EXISTS mcp_interaction_state ON TABLE mcp_interaction FIELDS server, state, expires_at;
DEFINE INDEX IF NOT EXISTS mcp_interaction_authority ON TABLE mcp_interaction FIELDS tenant, owner, work_context, profile, expires_at;
DEFINE INDEX IF NOT EXISTS mcp_interaction_task ON TABLE mcp_interaction FIELDS task;

-- The legacy server-initiated elicitation table has no final-profile caller.
REMOVE TABLE IF EXISTS agent_elicitation;

-- Replace the old wake discriminator in place. Existing historical rows keep
-- their payload, while every new multi-round wake uses the final application
-- input-request name.
UPDATE wake SET kind = "input_request" WHERE kind = "elicitation";
DEFINE FIELD OVERWRITE kind ON TABLE wake TYPE "task_result" | "resource_changed" | "timer" | "operator_message" | "input_request";

-- Agent-facing form input is application state for Rig deferred executions.
-- It is deliberately named independently of the removed MCP elicitation wire
-- feature and is addressed by an opaque input-request identifier.
DEFINE TABLE IF NOT EXISTS agent_input_request SCHEMAFULL TYPE NORMAL CHANGEFEED 30d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE agent_input_request TYPE uuid DEFAULT rand::uuid();
DEFINE FIELD IF NOT EXISTS tenant ON TABLE agent_input_request TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS agent ON TABLE agent_input_request TYPE record<agent>;
DEFINE FIELD IF NOT EXISTS related_task ON TABLE agent_input_request TYPE option<string>;
DEFINE FIELD IF NOT EXISTS message ON TABLE agent_input_request TYPE string;
DEFINE FIELD IF NOT EXISTS requested_schema ON TABLE agent_input_request TYPE option<object> FLEXIBLE;
DEFINE FIELD IF NOT EXISTS state ON TABLE agent_input_request TYPE "pending" | "answered" | "declined" | "cancelled" DEFAULT "pending";
DEFINE FIELD IF NOT EXISTS answer ON TABLE agent_input_request TYPE option<object> FLEXIBLE;
DEFINE FIELD IF NOT EXISTS answered_by ON TABLE agent_input_request TYPE option<string>;
DEFINE FIELD IF NOT EXISTS requested_at ON TABLE agent_input_request TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS answered_at ON TABLE agent_input_request TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS revision ON TABLE agent_input_request TYPE int DEFAULT 0 ASSERT $value >= 0;
DEFINE INDEX IF NOT EXISTS agent_input_request_state ON TABLE agent_input_request FIELDS agent, state, requested_at;
