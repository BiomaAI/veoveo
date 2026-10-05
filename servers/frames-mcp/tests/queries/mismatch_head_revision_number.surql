UPDATE frame_world SET head_revision_key = $new_key, revision = 1
WHERE tenant = $tenant AND world_key = $world_key
RETURN NONE;
