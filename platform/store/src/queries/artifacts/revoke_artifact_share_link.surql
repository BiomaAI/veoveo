UPDATE ONLY $record SET revoked_at = time::now() WHERE artifact = $artifact AND revoked_at = NONE RETURN AFTER;
