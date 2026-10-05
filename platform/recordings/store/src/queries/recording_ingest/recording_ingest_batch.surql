SELECT * FROM recording_ingest_batch WHERE tenant = $tenant AND stream = $stream AND sequence = $sequence LIMIT 1;
