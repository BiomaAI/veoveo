-- Producer quota accounting is part of the same transaction that accepts a
-- recording batch. The indexed producer snapshot avoids policy lookups and
-- record traversal while the gateway remains the policy source of truth.

DEFINE FIELD IF NOT EXISTS producer_id ON TABLE recording_ingest_batch TYPE option<string>;
DEFINE INDEX IF NOT EXISTS recording_ingest_batch_producer_time ON TABLE recording_ingest_batch FIELDS tenant, producer_id, created_at;
