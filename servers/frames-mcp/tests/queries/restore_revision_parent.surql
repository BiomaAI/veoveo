UPDATE frame_world_revision SET world = (SELECT VALUE id FROM frame_world
WHERE tenant = $tenant AND world_key = $world_key
LIMIT 1)[0]
WHERE tenant = $tenant AND revision_key = $revision_key
RETURN NONE;
