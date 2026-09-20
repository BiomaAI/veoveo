LET $grants = SELECT VALUE principal_key FROM uav_vehicle_control_grant
    WHERE tenant = $tenant AND work_context = $context AND session_id = $simulation_session
        AND revoked_at = NONE AND valid_from <= time::now()
        AND (valid_until = NONE OR valid_until > time::now())
    LIMIT 513;
IF array::len($grants) > 512 { THROW 'UAV message target grant inventory exceeds its bound'; };
SELECT VALUE key FROM managed_agent
    WHERE tenant = $tenant AND work_context = $context
        AND active_generation > 0 AND public_key != NONE
        AND observed IN ['ready', 'paused']
        AND identity.issuer + '#' + identity.client_id IN $grants
        AND fn::managed_agent_enabled(id)
    ORDER BY key ASC LIMIT 32;
