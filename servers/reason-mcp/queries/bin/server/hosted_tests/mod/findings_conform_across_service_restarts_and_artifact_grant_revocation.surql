UPDATE artifact_grant SET expires_at = $expiry WHERE in = $artifact AND out = $principal RETURN NONE;
