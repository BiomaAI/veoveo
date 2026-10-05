UPDATE frame_world_revision SET revision_key = 'other'
WHERE world_key = $world
RETURN NONE;
