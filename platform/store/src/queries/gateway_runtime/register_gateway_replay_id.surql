UPSERT ONLY $record CONTENT $content WHERE expires_at = NONE OR expires_at <= $now RETURN AFTER;
