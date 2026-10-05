UPDATE ONLY $event SET processing_error = $error
WHERE processed_at = NONE AND provider_job = $job AND tenant = $tenant AND provider = $provider RETURN NONE;
COMMIT TRANSACTION;
