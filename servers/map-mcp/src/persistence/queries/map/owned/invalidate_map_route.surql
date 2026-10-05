UPDATE ONLY $record SET status = 'invalidated', canonical_json = $canonical_json, updated_at = time::now() WHERE tenant = $tenant AND status != 'invalidated' RETURN AFTER;
