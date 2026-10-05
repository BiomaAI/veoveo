RETURN count((SELECT VALUE id FROM media_usage
WHERE task = $task AND tenant = task.tenant AND task.server = mcp_server:media AND provider_job.task = task AND provider_job.tenant = tenant AND provider_job.provider = $provider AND provider_job.external_job_id = $prediction AND provider_job.provider_payload.id = $prediction AND kind = 'actual'
LIMIT 1)) > 0;
