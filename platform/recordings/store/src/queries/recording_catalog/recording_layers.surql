SELECT * FROM recording_layer WHERE tenant = $tenant AND recording = $recording ORDER BY ordinal ASC, layer_name ASC LIMIT $limit;
