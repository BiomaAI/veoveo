DELETE gateway_jwt_revocation WHERE expires_at <= $now RETURN BEFORE;
