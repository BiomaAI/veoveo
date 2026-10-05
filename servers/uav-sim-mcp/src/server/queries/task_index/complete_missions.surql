SELECT mission_id, array::group(plan_id) AS plan_ids FROM uav_vehicle_mission_plan
    WHERE tenant = $tenant AND work_context = $context AND principal_key = $principal_key
        AND id IN $admitted_plans AND string::contains(string::lowercase(mission_id), $needle)
    GROUP BY mission_id ORDER BY mission_id ASC LIMIT $limit;
