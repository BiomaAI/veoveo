UPDATE frame_world_revision SET frame_ids=['root','invented'] WHERE tenant=$tenant AND revision_key=$revision_key RETURN NONE;
