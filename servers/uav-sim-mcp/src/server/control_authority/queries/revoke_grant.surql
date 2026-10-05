UPDATE ONLY $record SET revoked_at = $now, revoked_by = $actor, updated_at = $now, revision += 1 WHERE revoked_at = NONE AND revision = $revision RETURN AFTER;
