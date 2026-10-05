DEFINE EVENT reject_settlement ON TABLE uav_vehicle_mission_plan WHEN $after.state = 'completed' THEN { THROW 'fixture rejects settlement'; };
