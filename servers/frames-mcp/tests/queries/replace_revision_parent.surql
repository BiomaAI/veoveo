UPDATE frame_world_revision SET world = (SELECT VALUE id FROM frame_world
WHERE tenant = $parent_tenant AND world_key = $parent_key
LIMIT 1)[0]
WHERE tenant = $tenant AND revision_key = $revision_key
RETURN NONE;
