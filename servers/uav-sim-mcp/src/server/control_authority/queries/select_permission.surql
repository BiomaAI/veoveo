SELECT * FROM uav_vehicle_control_grant WHERE tenant = $tenant AND work_context = $work_context
    AND principal_key = $principal AND session_id = $simulation_session AND vehicle_id = $vehicle
    AND revoked_at = NONE AND valid_from <= $now AND (valid_until = NONE OR valid_until > $now)
    AND permissions CONTAINSALL $permissions
    AND ($profile = NONE OR map_mobility_profile_uri = $profile)
    AND ($advisory = false OR allow_planning_advisory = true)
             ORDER BY created_at ASC, grant_id ASC LIMIT 1;
