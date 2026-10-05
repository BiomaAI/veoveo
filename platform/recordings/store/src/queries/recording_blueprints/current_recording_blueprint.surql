SELECT * FROM recording_blueprint WHERE tenant = $tenant AND recording = $recording ORDER BY revision DESC LIMIT 1;
