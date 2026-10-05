DELETE frame_world
WHERE tenant = $tenant AND world_key = $world
RETURN NONE;
