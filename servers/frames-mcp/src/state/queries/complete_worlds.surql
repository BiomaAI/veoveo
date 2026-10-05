SELECT VALUE world_key FROM frame_world
WHERE tenant = $tenant AND $clearance CONTAINSALL labels AND string::contains(string::lowercase(world_key), $needle)
ORDER BY world_key ASC
LIMIT $limit;
