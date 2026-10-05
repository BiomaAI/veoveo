UPDATE ONLY $stream SET state = 'finished', finished_at = $now, updated_at = $now, revision += 1 WHERE tenant = $tenant AND state = 'open' AND revision = $revision RETURN NONE;
