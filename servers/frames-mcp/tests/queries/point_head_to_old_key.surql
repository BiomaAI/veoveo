UPDATE frame_world SET head_revision_key = $old_key
WHERE tenant = $tenant AND world_key = $world_key
RETURN NONE;
