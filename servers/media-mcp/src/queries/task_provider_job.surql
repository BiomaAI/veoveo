SELECT * FROM provider_job
WHERE provider = $provider AND task = $task
ORDER BY submitted_at ASC
LIMIT 1;
