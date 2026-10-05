SELECT * FROM ONLY $grant WHERE connection_id = $connection AND provider_instance_id = $provider
                 AND revoked_at = NONE AND expires_at > time::now() AND idle_expires_at > time::now()
                 AND family.revoked_at = NONE AND family.expires_at > time::now();
                 SELECT * FROM ONLY $policy; RETURN time::now();
