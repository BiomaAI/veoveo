SELECT VALUE definition.frames[WHERE frame_id = $frame_key]
                FROM frame_world_revision WHERE tenant = $tenant AND world_key = $world_key
    AND world.tenant = $tenant AND world.world_key = $world_key
    AND owner = world.owner AND $clearance CONTAINSALL world.labels
                AND revision_key = $revision_key LIMIT 1;
