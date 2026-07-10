-- Governed recording catalog and atomic seal publication. Existing recording
-- development data is intentionally not migrated: this is a hard-cut schema.

DELETE recording;
DELETE segment;
DELETE recording_segment;

DEFINE FIELD IF NOT EXISTS dataset ON TABLE recording TYPE string;
DEFINE FIELD OVERWRITE state ON TABLE recording TYPE "open" | "sealing" | "sealed" | "failed";
DEFINE FIELD IF NOT EXISTS manifest_artifact ON TABLE recording TYPE option<record<artifact_occurrence>>;
DEFINE FIELD IF NOT EXISTS seal_task ON TABLE recording TYPE option<record<task>>;
DEFINE FIELD IF NOT EXISTS failure_reason ON TABLE recording TYPE option<string>;
DEFINE FIELD IF NOT EXISTS updated_at ON TABLE recording TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS revision ON TABLE recording TYPE int DEFAULT 0 ASSERT $value >= 0;
DEFINE INDEX IF NOT EXISTS recording_tenant_dataset_started ON TABLE recording FIELDS tenant, dataset, started_at;

REMOVE FIELD IF EXISTS object_key ON TABLE segment;
DEFINE FIELD IF NOT EXISTS relative_path ON TABLE segment TYPE string;
DEFINE FIELD OVERWRITE state ON TABLE segment TYPE "writing" | "frozen" | "sealed" | "failed";
DEFINE FIELD IF NOT EXISTS message_count ON TABLE segment TYPE int DEFAULT 0 ASSERT $value >= 0;
DEFINE FIELD IF NOT EXISTS failure_reason ON TABLE segment TYPE option<string>;
DEFINE FIELD IF NOT EXISTS updated_at ON TABLE segment TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS revision ON TABLE segment TYPE int DEFAULT 0 ASSERT $value >= 0;
REMOVE INDEX IF EXISTS segment_object_key_unique ON TABLE segment;
DEFINE INDEX IF NOT EXISTS segment_tenant_path_unique ON TABLE segment FIELDS tenant, relative_path UNIQUE;
