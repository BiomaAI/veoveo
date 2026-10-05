DELETE frame_world
WHERE tenant = $tenant AND world_key = $world_key
RETURN NONE;
