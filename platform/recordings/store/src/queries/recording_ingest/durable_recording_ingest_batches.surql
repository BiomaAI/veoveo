SELECT * FROM recording_ingest_batch WHERE tenant = $tenant AND stream = $stream AND state = 'durable' ORDER BY sequence ASC LIMIT $limit;
