UPDATE frame_world_revision SET world = frame_world:other
WHERE world_key = $world
RETURN NONE;
