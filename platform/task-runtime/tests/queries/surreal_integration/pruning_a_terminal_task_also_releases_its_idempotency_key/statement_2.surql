SELECT VALUE id FROM task_input WHERE task = $task; SELECT VALUE id FROM task_idempotency WHERE task = $task;
