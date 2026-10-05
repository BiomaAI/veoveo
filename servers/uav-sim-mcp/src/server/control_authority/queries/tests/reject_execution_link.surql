DEFINE EVENT reject_link ON TABLE uav_mission_execution WHEN $event = 'CREATE' THEN { THROW 'native link failure'; };
