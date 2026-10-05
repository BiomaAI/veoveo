UPDATE ONLY $event SET processed_at = time::now(), processing_error = $error
WHERE processed_at = NONE
RETURN NONE;
