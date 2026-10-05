UPDATE $grant SET expires_at = time::now() - 1s RETURN NONE;
