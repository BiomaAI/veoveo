UPDATE $grant SET expires_at = time::now() + 1m RETURN NONE;
