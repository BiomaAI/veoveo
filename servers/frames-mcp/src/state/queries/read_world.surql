SELECT * FROM frame_world
WHERE tenant = $tenant AND world_key = $world_key AND $clearance CONTAINSALL labels
LIMIT 1;
