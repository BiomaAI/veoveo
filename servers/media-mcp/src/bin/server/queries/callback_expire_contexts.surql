UPDATE media_task_context SET capability_expires_at = time::now() - 1s WHERE task IN $tasks;
