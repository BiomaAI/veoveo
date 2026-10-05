
        FOR $instance IN (SELECT * FROM managed_agent) {
            LET $old = array::first(SELECT * FROM agent_definition WHERE key = $instance.key);
            UPDATE ONLY $instance.id SET definition = $old.id, requested_revision = $old.published,
                active_revision = $old.published, desired = 'paused', observed = 'paused';
        };
        DELETE agent_definition WHERE key = 'uav-pilot';
        UPDATE agent_definition SET disabled = true, status = 'disabled';
        UPDATE agent SET lease_expires_at = NONE, lease_owner = NONE, state = 'idle', last_episode = NONE;
