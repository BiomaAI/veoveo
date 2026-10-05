UPDATE ONLY $task SET retention_expires_at = time::now() - 1s RETURN NONE;
