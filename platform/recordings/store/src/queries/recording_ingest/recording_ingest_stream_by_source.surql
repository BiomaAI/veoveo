SELECT * FROM recording_ingest_stream WHERE tenant = $tenant AND producer_id = $producer_id AND source_stream_id = $source_stream_id LIMIT 1;
