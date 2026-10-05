SELECT grant_id AS value FROM uav_vehicle_control_grant WHERE tenant = $tenant AND work_context = $work_context AND ($include_all OR principal_key = $principal)
             AND string::contains(string::lowercase(grant_id), $needle) ORDER BY value ASC LIMIT $limit;
