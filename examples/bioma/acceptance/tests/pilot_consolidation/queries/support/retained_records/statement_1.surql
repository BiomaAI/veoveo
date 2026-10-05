
        LET $instances = SELECT * FROM managed_agent WHERE tenant = $tenant AND key IN $keys;
        LET $ids = array::distinct(array::flatten($instances.map(|$i| [$i.id, $i.operation, $i.definition,
            $i.requested_revision, $i.principal, $i.owner, $i.tenant, $i.work_context])));
        SELECT * FROM $ids;
        SELECT * FROM agent WHERE tenant = $tenant AND agent_key IN $keys;
        SELECT * FROM uav_vehicle_control_grant WHERE tenant = $tenant AND principal_key IN $principals;
        SELECT * FROM agent_definition WHERE tenant = $tenant AND key IN $keys;
        SELECT * FROM agent_definition_revision WHERE definition.tenant = $tenant AND definition.key IN $keys;
