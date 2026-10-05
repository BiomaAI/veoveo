UPDATE frame_world_revision SET spec_sha256 = $digest, definition.frames[0].description = 'tampered'
WHERE tenant = $tenant AND revision_key = $revision_key
RETURN NONE;
