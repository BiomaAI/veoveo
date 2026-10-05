UPDATE agent SET lease_expires_at = time::now() - 1s WHERE tenant = $tenant AND agent_key = 'one';
