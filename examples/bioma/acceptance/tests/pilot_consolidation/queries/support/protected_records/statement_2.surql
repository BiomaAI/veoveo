
        SELECT * FROM principal WHERE id IN (SELECT VALUE principal FROM managed_agent WHERE tenant = $tenant AND key IN $keys);
        SELECT * FROM agent WHERE tenant = $tenant AND agent_key IN $keys;
        SELECT * FROM uav_vehicle_control_grant WHERE tenant = $tenant AND principal_key IN $principals;
