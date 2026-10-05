SELECT * FROM recording_layer WHERE tenant = $tenant AND recording = $recording AND layer_name = $layer_name LIMIT 1;
