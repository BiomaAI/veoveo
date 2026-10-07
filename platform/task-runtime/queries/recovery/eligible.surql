SELECT * FROM $records
WHERE server = $server AND status IN ['queued', 'running', 'waiting', 'cancel_requested']
    AND (lease_expires_at = NONE OR lease_expires_at <= $now);
