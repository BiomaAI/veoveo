SELECT * FROM ONLY $grant WHERE provider_instance_id = $provider AND credential_hash = $credential_hash
                AND authority.profile = $profile AND ($computer = NONE OR computer_id = $computer)
                AND revoked_at = NONE AND expires_at > time::now() AND idle_expires_at > time::now()
                AND fn::kernel::gateway::refresh_family_current_v1(family, time::now()) != NONE;
