UPDATE ONLY $recording SET revision += 1, updated_at = time::now() RETURN NONE;
