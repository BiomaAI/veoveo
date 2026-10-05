UPDATE ONLY $task SET lease_expires_at = time::now() - 1s
WHERE status = 'waiting' AND lease_owner = $worker AND type::string(lease_expires_at) = $expiry;
