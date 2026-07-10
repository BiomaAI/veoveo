-- Durable coordinate operation provenance keyed by the public UUIDv7 operation id.

DEFINE TABLE IF NOT EXISTS coordinate_operation SCHEMAFULL TYPE NORMAL CHANGEFEED 30d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE coordinate_operation TYPE uuid;
DEFINE FIELD IF NOT EXISTS tenant ON TABLE coordinate_operation TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS owner ON TABLE coordinate_operation TYPE record<principal>;
DEFINE FIELD IF NOT EXISTS task ON TABLE coordinate_operation TYPE option<record<task>>;
DEFINE FIELD IF NOT EXISTS operation_key ON TABLE coordinate_operation TYPE string;
DEFINE FIELD IF NOT EXISTS kind ON TABLE coordinate_operation TYPE string;
DEFINE FIELD IF NOT EXISTS provenance ON TABLE coordinate_operation TYPE object FLEXIBLE;
DEFINE FIELD IF NOT EXISTS classification ON TABLE coordinate_operation TYPE string;
DEFINE FIELD IF NOT EXISTS labels ON TABLE coordinate_operation TYPE array<string> DEFAULT [];
DEFINE FIELD IF NOT EXISTS created_at ON TABLE coordinate_operation TYPE datetime;
DEFINE FIELD IF NOT EXISTS updated_at ON TABLE coordinate_operation TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS coordinate_operation_tenant_key_unique ON TABLE coordinate_operation FIELDS tenant, operation_key UNIQUE;
DEFINE INDEX IF NOT EXISTS coordinate_operation_task ON TABLE coordinate_operation FIELDS task;
DEFINE INDEX IF NOT EXISTS coordinate_operation_owner_time ON TABLE coordinate_operation FIELDS tenant, owner, created_at;
