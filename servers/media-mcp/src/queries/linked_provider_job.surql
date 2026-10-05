SELECT * FROM provider_job
WHERE provider = $provider AND task = $task AND task.server = mcp_server:media AND tenant = task.tenant AND external_job_id = $prediction AND provider_payload.id = $prediction
LIMIT 1;
