SELECT * FROM recording WHERE tenant = $tenant AND application_id = $application_id AND recording_key = $recording_key LIMIT 1;
