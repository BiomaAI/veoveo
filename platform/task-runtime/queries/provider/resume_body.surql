IF $_provider_guard.status != $_resume_status OR $_provider_guard.updated_at != $_resume_updated OR $_provider_guard.cancel_requested_at != $_resume_cancelled { THROW 'provider_lease_lost'; }; /* domain body */
UPDATE ONLY $_provider_task SET status = 'waiting', request = $_resume_request, updated_at = $_resume_now;
