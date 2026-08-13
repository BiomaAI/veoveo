-- Public MCP task identifiers are opaque gateway routes, not database UUIDs or
-- provider identifiers. The authority digest binds the route to the exact
-- tenant, actor, Work Context, profile, and source server checked on every use.

DEFINE TABLE IF NOT EXISTS gateway_task_route SCHEMAFULL TYPE NORMAL CHANGEFEED 30d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS tenant ON TABLE gateway_task_route TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS owner ON TABLE gateway_task_route TYPE record<principal>;
DEFINE FIELD IF NOT EXISTS work_context ON TABLE gateway_task_route TYPE record<work_context>;
DEFINE FIELD IF NOT EXISTS profile ON TABLE gateway_task_route TYPE record<profile>;
DEFINE FIELD IF NOT EXISTS server ON TABLE gateway_task_route TYPE record<mcp_server>;
DEFINE FIELD IF NOT EXISTS source_task_id ON TABLE gateway_task_route TYPE string ASSERT string::len($value) > 0 AND string::len($value) <= 512;
DEFINE FIELD IF NOT EXISTS authority_digest ON TABLE gateway_task_route TYPE string ASSERT string::len($value) = 64;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE gateway_task_route TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS expires_at ON TABLE gateway_task_route TYPE datetime;
DEFINE INDEX IF NOT EXISTS gateway_task_route_source ON TABLE gateway_task_route FIELDS server, source_task_id, owner, profile UNIQUE;
DEFINE INDEX IF NOT EXISTS gateway_task_route_authority ON TABLE gateway_task_route FIELDS tenant, owner, work_context, profile, expires_at;
