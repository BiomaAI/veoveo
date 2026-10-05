UPDATE frame_world_revision SET owner = principal:other
WHERE world_key = $world
RETURN NONE;
