SELECT VALUE revision_key FROM frame_world_revision WHERE tenant = $tenant AND world_key = $world_key
    AND world.tenant = $tenant AND world.world_key = $world_key
    AND owner = world.owner AND $clearance CONTAINSALL world.labels AND string::contains(string::lowercase(revision_key), $needle) ORDER BY revision_key ASC LIMIT $limit;
