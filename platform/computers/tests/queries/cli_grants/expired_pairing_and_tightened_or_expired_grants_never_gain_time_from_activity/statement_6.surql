UPDATE ONLY $grant SET idle_expires_at = time::now() + 1m, expires_at = time::now() - 1s;
