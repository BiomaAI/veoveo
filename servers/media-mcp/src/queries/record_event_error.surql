UPDATE ONLY $event SET processing_error = $error
WHERE processed_at = NONE
RETURN NONE;
