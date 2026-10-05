SELECT * FROM frame_world_revision WHERE tenant = $tenant AND world_key = $world_key
    AND world.tenant = $tenant AND world.world_key = $world_key
    AND owner = world.owner AND $clearance CONTAINSALL world.labels
                AND id = world.head_revision AND revision_key = world.head_revision_key
                AND revision = world.revision LIMIT 1;
