UPDATE artifact_grant SET expires_at = $expiry WHERE in = $artifact AND subject_key = 'reader' RETURN NONE;
