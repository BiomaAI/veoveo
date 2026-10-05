SELECT * FROM ONLY $grant WHERE connection_id = $connection AND provider_instance_id = $provider
                 AND revoked_at = NONE AND expires_at > time::now() AND idle_expires_at > time::now()
                 AND fn::kernel::gateway::refresh_family_current_v1(family, time::now()) != NONE;
                 SELECT * FROM ONLY $policy; RETURN time::now();
