BEGIN TRANSACTION;
             LET $_admission_guard = (UPDATE ONLY $_admission_task SET updated_at = updated_at
               WHERE server = $_admission_server AND tenant = $_admission_tenant
                 AND owner = $_admission_owner AND work_context = $_admission_context
                 AND profile = $_admission_profile AND recovery_class = $_admission_class
                 AND task_type = $_admission_kind
                 AND status = 'queued' AND lease_owner = NONE AND lease_expires_at = NONE
                 AND cancel_requested_at = NONE AND updated_at = $_admission_updated
                 AND request = $_admission_request AND owner_context = $_admission_owner_context AND retention_pins = $_admission_pins RETURN AFTER);
             IF $_admission_guard = NONE { THROW 'task_admission_conflict'; };
             /* domain body */
COMMIT TRANSACTION;
