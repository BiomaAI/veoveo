UPDATE ONLY $event SET processed_at = $now, processing_error = NONE WHERE processed_at = NONE RETURN NONE;
RETURN $_webhook_updated;
COMMIT TRANSACTION;
