SELECT * FROM ONLY $grant WHERE provider_instance_id = $provider AND revoked_at = NONE
                AND expires_at > time::now() AND idle_expires_at > time::now()
                AND fn::kernel::gateway::refresh_family_current_v1(family, time::now()) != NONE
                AND $connection.grant_id = $grant_id AND $connection.connection_id = $connection_id
                AND $connection.closed_at = NONE AND $connection.expires_at > time::now();
                SELECT * FROM ONLY $connection WHERE grant_id = $grant_id AND connection_id = $connection_id
                AND closed_at = NONE AND expires_at > time::now();
                SELECT * FROM ONLY $policy; RETURN time::now();
