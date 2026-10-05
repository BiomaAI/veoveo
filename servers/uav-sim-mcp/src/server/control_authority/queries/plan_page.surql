SELECT * FROM uav_vehicle_mission_plan WHERE tenant = $tenant AND work_context = $work_context AND ($include_all OR principal_key = $principal)
             AND ($after = NONE OR plan_id > $after) ORDER BY plan_id ASC LIMIT $limit;
