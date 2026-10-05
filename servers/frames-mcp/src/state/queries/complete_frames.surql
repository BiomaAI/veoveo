SELECT VALUE frame_id FROM (SELECT VALUE definition.frames FROM frame_world_revision WHERE tenant = $tenant AND world_key = $world_key
    AND world.tenant = $tenant AND world.world_key = $world_key
    AND owner = world.owner AND $clearance CONTAINSALL world.labels AND revision_key = $revision_key LIMIT 1)[0] WHERE string::contains(string::lowercase(frame_id), $needle) ORDER BY frame_id ASC LIMIT $limit;
