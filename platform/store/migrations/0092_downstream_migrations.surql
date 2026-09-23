DEFINE TABLE IF NOT EXISTS platform_downstream_migration SCHEMAFULL TYPE NORMAL
  PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE platform_downstream_migration TYPE int;
DEFINE FIELD IF NOT EXISTS version ON TABLE platform_downstream_migration TYPE int
  READONLY;
DEFINE FIELD IF NOT EXISTS name ON TABLE platform_downstream_migration TYPE string
  READONLY;
DEFINE FIELD IF NOT EXISTS filename ON TABLE platform_downstream_migration TYPE string
  READONLY;
DEFINE FIELD IF NOT EXISTS checksum ON TABLE platform_downstream_migration TYPE string
  READONLY;
DEFINE FIELD IF NOT EXISTS requires_upstream ON TABLE platform_downstream_migration TYPE int
  READONLY;
DEFINE FIELD IF NOT EXISTS applied_at ON TABLE platform_downstream_migration TYPE datetime
  DEFAULT time::now()
  READONLY;
DEFINE INDEX IF NOT EXISTS platform_downstream_migration_version ON TABLE platform_downstream_migration FIELDS version
  UNIQUE;
