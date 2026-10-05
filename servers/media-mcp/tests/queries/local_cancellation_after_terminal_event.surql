UPDATE ONLY $task SET status = 'cancelled' RETURN NONE;
UPDATE media_task SET settlement = {status: 'cancelled', completed_at: time::now()} WHERE task = $task RETURN NONE;
