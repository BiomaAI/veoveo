SELECT * FROM provider_job
WHERE provider = $provider AND task.server = mcp_server:media AND tenant = task.tenant AND external_job_id = provider_payload.id AND provider_payload.status IN ['completed', 'failed'] AND (SELECT VALUE id FROM media_usage
WHERE task = $parent.task AND tenant = $parent.tenant AND provider_job = $parent.id AND kind = 'actual'
LIMIT 1) = [] AND id > $after AND id != $after
ORDER BY id ASC
LIMIT 101;
