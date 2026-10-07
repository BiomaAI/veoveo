SELECT id, lease_expires_at FROM task
WHERE server = $server AND status IN ['queued', 'running', 'waiting', 'cancel_requested'];
