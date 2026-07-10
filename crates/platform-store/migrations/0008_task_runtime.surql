-- Durable idempotency claims for canonical task creation.
--
-- The record key is a SHA-256 digest over tenant, owner, profile, server, and
-- the caller-provided key. CREATE ONLY makes concurrent submissions converge
-- on exactly one UUIDv7 task without placing optional values in a unique index.

DEFINE TABLE IF NOT EXISTS task_idempotency SCHEMAFULL TYPE NORMAL CHANGEFEED 30d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE task_idempotency TYPE string;
DEFINE FIELD IF NOT EXISTS task ON TABLE task_idempotency TYPE record<task>;
DEFINE FIELD IF NOT EXISTS tenant ON TABLE task_idempotency TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS owner ON TABLE task_idempotency TYPE record<principal>;
DEFINE FIELD IF NOT EXISTS server ON TABLE task_idempotency TYPE record<mcp_server>;
DEFINE FIELD IF NOT EXISTS key ON TABLE task_idempotency TYPE string ASSERT string::len($value) > 0;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE task_idempotency TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS task_idempotency_task ON TABLE task_idempotency FIELDS task UNIQUE;
DEFINE INDEX IF NOT EXISTS task_idempotency_owner_server ON TABLE task_idempotency FIELDS tenant, owner, server, created_at;
