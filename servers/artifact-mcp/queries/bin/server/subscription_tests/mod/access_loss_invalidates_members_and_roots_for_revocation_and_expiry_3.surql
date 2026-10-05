UPDATE $artifact SET retention_expires_at = $expiry RETURN NONE; UPDATE artifact_grant SET expires_at = $expiry WHERE in = $artifact AND subject_key = 'author' RETURN NONE;
