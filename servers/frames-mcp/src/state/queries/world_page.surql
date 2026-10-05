SELECT * FROM frame_world
WHERE tenant = $tenant AND $clearance CONTAINSALL labels AND ($after = NONE OR world_key > $after)
ORDER BY world_key ASC
LIMIT $limit;
