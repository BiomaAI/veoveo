SELECT * FROM recording_blueprint WHERE tenant = $tenant AND recording = $recording AND revision = $revision LIMIT 1;
