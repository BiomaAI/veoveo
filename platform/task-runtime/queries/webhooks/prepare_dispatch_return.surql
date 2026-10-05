IF $_dispatch_new {
    IF $_dispatch_task.status NOT IN ['queued','running','waiting']
        OR $_dispatch_task.cancel_requested_at != NONE
        OR $_dispatch_task.lease_owner != $worker OR $_dispatch_task.lease_expires_at <= $now {
        THROW 'task_dispatch_fence_conflict';
    };
    UPDATE ONLY $task SET retention_pins += $provider_retention_pin,
        status = 'waiting', request.status_message = 'provider dispatch prepared; awaiting authenticated outcome',
        lease_owner = NONE, lease_expires_at = NONE, updated_at = $now;
};
RETURN $_dispatch_new;
COMMIT TRANSACTION;
