-- Canonical gateway Task routes retain the protocol's opaque upstream Task ID.
-- First-party shared-runtime Tasks also carry a strong record reference so
-- agent retention pins can be released in the consuming transaction.

DEFINE FIELD IF NOT EXISTS source_task ON TABLE gateway_task_route TYPE option<record<task>>;
UPDATE gateway_task_route
SET source_task = IF string::is_uuid(source_task_id) {
    type::record('task', type::uuid(source_task_id))
} ELSE {
    NONE
}
RETURN NONE;
