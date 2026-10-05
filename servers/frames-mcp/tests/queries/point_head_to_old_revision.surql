UPDATE frame_world SET revision = 2, head_revision = (SELECT VALUE id FROM frame_world_revision
WHERE tenant = $tenant AND world_key = $world_key AND revision_key = $old_key
LIMIT 1)[0]
WHERE tenant = $tenant AND world_key = $world_key
RETURN NONE;
