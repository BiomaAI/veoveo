UPDATE frame_world_revision SET root_frame_key = 'missing'
WHERE tenant = $tenant AND revision_key = $revision_key
RETURN NONE;
