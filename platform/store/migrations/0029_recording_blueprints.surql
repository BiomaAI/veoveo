-- Immutable producer-authored Rerun Blueprint revisions associated with one
-- governed recording. The recording remains the authorization and retention
-- boundary; Blueprint bytes remain distinct presentation stores.

DEFINE TABLE IF NOT EXISTS recording_blueprint SCHEMAFULL TYPE NORMAL
  CHANGEFEED 30d INCLUDE ORIGINAL
  PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE recording_blueprint TYPE uuid
  DEFAULT rand::uuid();
DEFINE FIELD IF NOT EXISTS tenant ON TABLE recording_blueprint TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS recording ON TABLE recording_blueprint TYPE record<recording>;
DEFINE FIELD IF NOT EXISTS stream ON TABLE recording_blueprint TYPE option<record<recording_ingest_stream>>;
DEFINE FIELD IF NOT EXISTS artifact ON TABLE recording_blueprint TYPE option<record<artifact_occurrence>>;
DEFINE FIELD IF NOT EXISTS owner ON TABLE recording_blueprint TYPE record<principal>;
DEFINE FIELD IF NOT EXISTS work_context ON TABLE recording_blueprint TYPE record<work_context>;
DEFINE FIELD IF NOT EXISTS producer_id ON TABLE recording_blueprint TYPE string;
DEFINE FIELD IF NOT EXISTS application_id ON TABLE recording_blueprint TYPE string;
DEFINE FIELD IF NOT EXISTS blueprint_id ON TABLE recording_blueprint TYPE string;
DEFINE FIELD IF NOT EXISTS revision ON TABLE recording_blueprint TYPE int
  ASSERT $value > 0;
DEFINE FIELD IF NOT EXISTS relative_path ON TABLE recording_blueprint TYPE string;
DEFINE FIELD IF NOT EXISTS sha256 ON TABLE recording_blueprint TYPE string
  ASSERT string::len($value) = 64;
DEFINE FIELD IF NOT EXISTS byte_len ON TABLE recording_blueprint TYPE int
  ASSERT $value > 0;
DEFINE FIELD IF NOT EXISTS message_count ON TABLE recording_blueprint TYPE int
  ASSERT $value > 0;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE recording_blueprint TYPE datetime
  DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS recording_blueprint_revision_unique ON TABLE recording_blueprint FIELDS tenant, recording, revision
  UNIQUE;
DEFINE INDEX IF NOT EXISTS recording_blueprint_current ON TABLE recording_blueprint FIELDS tenant, recording, revision;
DEFINE INDEX IF NOT EXISTS recording_blueprint_stream ON TABLE recording_blueprint FIELDS stream, revision;
