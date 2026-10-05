SELECT * FROM recording_layer WHERE state IN ['writing', 'staged'] ORDER BY updated_at ASC LIMIT $limit;
