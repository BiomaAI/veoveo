UPDATE frame_world SET labels += 'secret'
WHERE world_key = 'world'
RETURN NONE;
