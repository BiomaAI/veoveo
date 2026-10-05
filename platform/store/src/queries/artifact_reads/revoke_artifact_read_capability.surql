UPDATE $capability SET revoked_at = time::now() WHERE tenant = $tenant AND actor = $actor RETURN AFTER;
