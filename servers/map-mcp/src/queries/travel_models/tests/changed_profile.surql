UPDATE ONLY $task SET profile = type::record('profile', 'changed-profile'), owner_context.profile = 'changed-profile' RETURN NONE;
