SELECT VALUE type::string(lease_expires_at) FROM ONLY $task
WHERE status = 'waiting' AND lease_owner = $worker AND updated_at = $revision;
