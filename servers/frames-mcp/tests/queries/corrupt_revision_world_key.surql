UPDATE frame_world_revision SET world_key = 'other'
WHERE world_key = $world
RETURN NONE;
