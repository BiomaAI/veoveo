SELECT VALUE id FROM uav_vehicle_mission_plan
WHERE tenant = $tenant AND work_context = $work_context
    AND session_id = $simulation_session AND vehicle_id = $vehicle AND state = 'executing'
LIMIT 1
