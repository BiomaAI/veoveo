-- Migration history is intentionally minimal and immutable. The runner applies
-- this definition before reading history, then records it as migration zero.
DEFINE TABLE IF NOT EXISTS platform_schema_migration SCHEMAFULL TYPE NORMAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE platform_schema_migration TYPE int;
DEFINE FIELD IF NOT EXISTS version ON TABLE platform_schema_migration TYPE int READONLY;
DEFINE FIELD IF NOT EXISTS name ON TABLE platform_schema_migration TYPE string READONLY;
DEFINE FIELD IF NOT EXISTS checksum ON TABLE platform_schema_migration TYPE string READONLY;
DEFINE FIELD IF NOT EXISTS applied_at ON TABLE platform_schema_migration TYPE datetime DEFAULT time::now() READONLY;
DEFINE INDEX IF NOT EXISTS platform_schema_migration_version ON TABLE platform_schema_migration FIELDS version UNIQUE;
