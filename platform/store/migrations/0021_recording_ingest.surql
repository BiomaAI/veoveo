-- Durable checkpoints for authenticated recording ingest. Journal files are
-- written and synced before these records advance the accepted sequence.

DEFINE TABLE IF NOT EXISTS recording_ingest_stream SCHEMAFULL TYPE NORMAL CHANGEFEED 30d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE recording_ingest_stream TYPE uuid DEFAULT rand::uuid();
DEFINE FIELD IF NOT EXISTS tenant ON TABLE recording_ingest_stream TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS owner ON TABLE recording_ingest_stream TYPE record<principal>;
DEFINE FIELD IF NOT EXISTS recording ON TABLE recording_ingest_stream TYPE record<recording>;
DEFINE FIELD IF NOT EXISTS producer_id ON TABLE recording_ingest_stream TYPE string;
DEFINE FIELD IF NOT EXISTS oauth_client_id ON TABLE recording_ingest_stream TYPE string;
DEFINE FIELD IF NOT EXISTS source_stream_id ON TABLE recording_ingest_stream TYPE string;
DEFINE FIELD IF NOT EXISTS application_id ON TABLE recording_ingest_stream TYPE string;
DEFINE FIELD IF NOT EXISTS recording_key ON TABLE recording_ingest_stream TYPE string;
DEFINE FIELD IF NOT EXISTS dataset ON TABLE recording_ingest_stream TYPE string;
DEFINE FIELD IF NOT EXISTS state ON TABLE recording_ingest_stream TYPE "open" | "finished" | "failed" DEFAULT "open";
DEFINE FIELD IF NOT EXISTS next_sequence ON TABLE recording_ingest_stream TYPE int DEFAULT 0 ASSERT $value >= 0;
DEFINE FIELD IF NOT EXISTS materialized_through_sequence ON TABLE recording_ingest_stream TYPE option<int> ASSERT $value = NONE OR $value >= 0;
DEFINE FIELD IF NOT EXISTS byte_len ON TABLE recording_ingest_stream TYPE int DEFAULT 0 ASSERT $value >= 0;
DEFINE FIELD IF NOT EXISTS message_count ON TABLE recording_ingest_stream TYPE int DEFAULT 0 ASSERT $value >= 0;
DEFINE FIELD IF NOT EXISTS failure_reason ON TABLE recording_ingest_stream TYPE option<string>;
DEFINE FIELD IF NOT EXISTS opened_at ON TABLE recording_ingest_stream TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS finished_at ON TABLE recording_ingest_stream TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS updated_at ON TABLE recording_ingest_stream TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS revision ON TABLE recording_ingest_stream TYPE int DEFAULT 0 ASSERT $value >= 0;
DEFINE INDEX IF NOT EXISTS recording_ingest_stream_source_unique ON TABLE recording_ingest_stream FIELDS tenant, producer_id, source_stream_id UNIQUE;
DEFINE INDEX IF NOT EXISTS recording_ingest_stream_recording ON TABLE recording_ingest_stream FIELDS recording, opened_at;
DEFINE INDEX IF NOT EXISTS recording_ingest_stream_state ON TABLE recording_ingest_stream FIELDS state, updated_at;

DEFINE TABLE IF NOT EXISTS recording_ingest_batch SCHEMAFULL TYPE NORMAL CHANGEFEED 30d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE recording_ingest_batch TYPE uuid DEFAULT rand::uuid();
DEFINE FIELD IF NOT EXISTS tenant ON TABLE recording_ingest_batch TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS stream ON TABLE recording_ingest_batch TYPE record<recording_ingest_stream>;
DEFINE FIELD IF NOT EXISTS sequence ON TABLE recording_ingest_batch TYPE int ASSERT $value >= 0;
DEFINE FIELD IF NOT EXISTS payload_format ON TABLE recording_ingest_batch TYPE string;
DEFINE FIELD IF NOT EXISTS sha256 ON TABLE recording_ingest_batch TYPE string;
DEFINE FIELD IF NOT EXISTS relative_path ON TABLE recording_ingest_batch TYPE string;
DEFINE FIELD IF NOT EXISTS byte_len ON TABLE recording_ingest_batch TYPE int ASSERT $value > 0;
DEFINE FIELD IF NOT EXISTS message_count ON TABLE recording_ingest_batch TYPE int ASSERT $value > 0;
DEFINE FIELD IF NOT EXISTS state ON TABLE recording_ingest_batch TYPE "durable" | "materialized" DEFAULT "durable";
DEFINE FIELD IF NOT EXISTS created_at ON TABLE recording_ingest_batch TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS materialized_at ON TABLE recording_ingest_batch TYPE option<datetime>;
DEFINE INDEX IF NOT EXISTS recording_ingest_batch_sequence_unique ON TABLE recording_ingest_batch FIELDS stream, sequence UNIQUE;
DEFINE INDEX IF NOT EXISTS recording_ingest_batch_journal_path_unique ON TABLE recording_ingest_batch FIELDS relative_path UNIQUE;
DEFINE INDEX IF NOT EXISTS recording_ingest_batch_materialization ON TABLE recording_ingest_batch FIELDS stream, state, sequence;
