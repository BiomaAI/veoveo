UPDATE media_task SET created_at = created_at + 1s WHERE task = $task RETURN NONE;
