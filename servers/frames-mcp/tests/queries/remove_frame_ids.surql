UPDATE frame_world_revision UNSET frame_ids WHERE tenant=$tenant AND revision_key=$revision_key RETURN NONE;
