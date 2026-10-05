SELECT vehicle_id FROM uav_vehicle_control_grant WHERE tenant = $tenant AND work_context = $work_context AND ($include_all OR principal_key = $principal) AND revoked_at = NONE AND valid_from <= $now AND (valid_until = NONE OR valid_until > $now)
             AND session_id = $simulation_session AND vehicle_id IN $vehicles AND permissions CONTAINS 'inspect'
             GROUP BY vehicle_id;
