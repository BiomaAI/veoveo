-- Shared durable identity for Map outputs formerly retained only in local DuckDB.
DEFINE TABLE IF NOT EXISTS map_derivation SCHEMAFULL TYPE NORMAL CHANGEFEED 90d PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE map_derivation TYPE array;
DEFINE FIELD IF NOT EXISTS tenant ON TABLE map_derivation TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS work_context ON TABLE map_derivation TYPE record<work_context>;
DEFINE FIELD IF NOT EXISTS kind ON TABLE map_derivation TYPE "raster" | "spatial";
DEFINE FIELD IF NOT EXISTS derivation_key ON TABLE map_derivation TYPE string;
DEFINE FIELD IF NOT EXISTS created_by ON TABLE map_derivation TYPE string;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE map_derivation TYPE datetime;
DEFINE FIELD IF NOT EXISTS canonical_json ON TABLE map_derivation TYPE string;
DEFINE INDEX IF NOT EXISTS map_derivation_scope_key ON TABLE map_derivation FIELDS tenant, work_context, kind, derivation_key UNIQUE;
