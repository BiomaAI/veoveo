DELETE gateway_authorization_request WHERE expires_at <= $now RETURN BEFORE; DELETE gateway_authorization_code WHERE expires_at <= $now RETURN BEFORE;
