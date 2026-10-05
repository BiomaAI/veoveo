UPDATE frame_world_revision SET revision = 9
WHERE world_key = $world
RETURN NONE;
