UPDATE ONLY $family SET revoked_at = time::now(), revocation_reason = 'logout';
