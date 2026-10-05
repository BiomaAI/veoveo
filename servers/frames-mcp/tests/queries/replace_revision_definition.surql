UPDATE frame_world_revision SET definition = $definition
WHERE tenant = $tenant AND revision_key = $revision_key
RETURN NONE;
