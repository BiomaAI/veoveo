UPDATE frame_world_revision SET root_frame_key = $root, spec_sha256 = $wrong_digest
WHERE tenant = $tenant AND revision_key = $revision_key
RETURN NONE;
