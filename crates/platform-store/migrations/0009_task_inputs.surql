-- Durable, lifetime-unique client input exchanges for final MCP tasks.

DEFINE TABLE IF NOT EXISTS task_input SCHEMAFULL TYPE NORMAL CHANGEFEED 30d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE task_input TYPE string;
DEFINE FIELD IF NOT EXISTS task ON TABLE task_input TYPE record<task>;
DEFINE FIELD IF NOT EXISTS request_key ON TABLE task_input TYPE string ASSERT string::len($value) > 0 AND string::len($value) <= 256;
DEFINE FIELD IF NOT EXISTS request ON TABLE task_input TYPE object FLEXIBLE;
DEFINE FIELD IF NOT EXISTS response ON TABLE task_input TYPE option<object> FLEXIBLE;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE task_input TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS responded_at ON TABLE task_input TYPE option<datetime>;
DEFINE INDEX IF NOT EXISTS task_input_task_key ON TABLE task_input FIELDS task, request_key UNIQUE;
DEFINE INDEX IF NOT EXISTS task_input_outstanding ON TABLE task_input FIELDS task, response, created_at;
