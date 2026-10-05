UPDATE frame_world SET labels = ['cui', 'mission', 'restricted']
WHERE tenant = $tenant AND world_key = $world_key
RETURN NONE;
