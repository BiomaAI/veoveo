UPDATE frame_world SET labels = ['cui', 'mission']
WHERE tenant = $tenant AND world_key > $after
RETURN NONE;
