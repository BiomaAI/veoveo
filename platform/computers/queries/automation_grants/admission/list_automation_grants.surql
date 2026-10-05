SELECT * FROM computer_automation_grant
                 WHERE owner_key = $owner_key AND computer_id = $computer_id
                   AND provider_instance_id = $provider AND revoked_at = NONE AND expires_at > time::now()
                 ORDER BY grant_id LIMIT 64;
