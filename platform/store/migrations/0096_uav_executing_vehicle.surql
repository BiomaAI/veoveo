-- UAV admission checks active vehicle ownership across principals and missions.
DEFINE INDEX IF NOT EXISTS uav_mission_plan_vehicle_state ON TABLE uav_vehicle_mission_plan
    FIELDS tenant, work_context, session_id, vehicle_id, state;
