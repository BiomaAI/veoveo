SELECT * FROM recording_layer WHERE tenant = $tenant AND state IN ['writing', 'staged'] ORDER BY updated_at ASC LIMIT $limit;
