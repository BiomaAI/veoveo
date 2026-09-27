-- UAV mission resources correlate admitted plans with caller-owned execution Tasks.
DEFINE INDEX IF NOT EXISTS uav_mission_plan_mission ON TABLE uav_vehicle_mission_plan FIELDS tenant, work_context, principal_key, mission_id;
DEFINE INDEX IF NOT EXISTS task_uav_plan ON TABLE task FIELDS server, request.input.plan_id;
