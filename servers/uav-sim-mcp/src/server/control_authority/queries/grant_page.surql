SELECT * FROM uav_vehicle_control_grant WHERE tenant = $tenant AND work_context = $work_context AND ($include_all OR principal_key = $principal)
             AND ($simulation_session = NONE OR (session_id = $simulation_session AND revoked_at = NONE AND valid_from <= $now AND (valid_until = NONE OR valid_until > $now)))
             AND ($after = NONE OR grant_id > $after) ORDER BY grant_id ASC LIMIT $limit;
