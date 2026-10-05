IF $_webhook_task.status NOT IN ['cancel_requested', 'cancelled'] {
    THROW 'task_webhook_cancellation_conflict';
};
UPDATE ONLY $job SET cancellation_receipt = $cancellation_receipt,
    state = IF terminal_event = NONE THEN 'cancel_requested' ELSE state END,
    updated_at = $now;
RETURN SELECT * FROM ONLY $job;
COMMIT TRANSACTION;
