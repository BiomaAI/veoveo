UPDATE ONLY $artifact SET release_state = $state, updated_at = time::now() RETURN AFTER;
