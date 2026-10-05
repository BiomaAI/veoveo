UPDATE ONLY $record SET consumed_at = $now, payload.consumed_at = $now WHERE expires_at > $now AND consumed_at = NONE RETURN BEFORE;
