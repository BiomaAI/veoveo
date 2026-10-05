RETURN array::len(SELECT id FROM audit_record WHERE activity = 'computer_restart_observed'
            AND actor_key = $actor AND target_ref = $computer);
