SELECT * FROM provider_job
WHERE tenant = $tenant AND provider = $provider AND external_job_id = $external_job_id
LIMIT 1;
