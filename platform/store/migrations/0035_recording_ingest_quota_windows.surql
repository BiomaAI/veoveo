-- Recording producer quotas use deterministic fixed UTC windows. The batch
-- ledger remains authoritative evidence; one bounded counter replaces a
-- historical aggregation on every accepted batch.

DEFINE TABLE IF NOT EXISTS recording_ingest_quota_window SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS id ON TABLE recording_ingest_quota_window TYPE uuid;
DEFINE FIELD IF NOT EXISTS tenant ON TABLE recording_ingest_quota_window TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS producer_id ON TABLE recording_ingest_quota_window TYPE string;
DEFINE FIELD IF NOT EXISTS period ON TABLE recording_ingest_quota_window TYPE "minute" | "day";
DEFINE FIELD IF NOT EXISTS window_started_at ON TABLE recording_ingest_quota_window TYPE datetime;
DEFINE FIELD IF NOT EXISTS window_ends_at ON TABLE recording_ingest_quota_window TYPE datetime;
DEFINE FIELD IF NOT EXISTS batch_count ON TABLE recording_ingest_quota_window TYPE int ASSERT $value >= 0;
DEFINE FIELD IF NOT EXISTS byte_len ON TABLE recording_ingest_quota_window TYPE int ASSERT $value >= 0;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE recording_ingest_quota_window TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS updated_at ON TABLE recording_ingest_quota_window TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS recording_ingest_quota_window_identity_unique ON TABLE recording_ingest_quota_window FIELDS tenant, producer_id, period, window_started_at UNIQUE;
DEFINE INDEX IF NOT EXISTS recording_ingest_quota_window_expiry ON TABLE recording_ingest_quota_window FIELDS tenant, producer_id, window_ends_at;
