DEFINE EVENT reject_admission ON TABLE uav_vehicle_mission_plan WHEN $after.state = 'executing' THEN { THROW 'fixture rejects admission'; };
