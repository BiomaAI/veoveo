SELECT * FROM ONLY $grant WHERE provider_instance_id = $provider AND revoked_at = NONE
                AND expires_at > time::now() AND idle_expires_at > time::now()
                AND family.revoked_at = NONE AND family.expires_at > time::now()
                AND $connection.grant_id = $grant_id AND $connection.connection_id = $connection_id
                AND $connection.closed_at = NONE AND $connection.expires_at > time::now();
                SELECT * FROM ONLY $connection WHERE grant_id = $grant_id AND connection_id = $connection_id
                AND closed_at = NONE AND expires_at > time::now();
                SELECT * FROM ONLY $policy; RETURN time::now();
