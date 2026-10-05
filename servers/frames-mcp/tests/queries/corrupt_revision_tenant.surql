UPDATE frame_world_revision SET tenant = tenant:other
WHERE world_key = $world
RETURN NONE;
