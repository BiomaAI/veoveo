SELECT VALUE receipt FROM (
SELECT id, received_at, {
    event: $this,
    job: (SELECT * FROM ONLY $this.provider_job),
    inserted: false,
    authoritative: provider_job.terminal_event = id
} AS receipt FROM provider_event
WHERE provider = $provider AND processed_at = NONE AND {
    LET $job = (SELECT * FROM ONLY $this.provider_job);
    LET $life = IF type::is_object($job) THEN fn::kernel::tasks::lifecycle_v1($job.task, $server, $job.tenant, $task_types, NONE) ELSE NONE END;
    IF type::is_object($job) AND type::is_object($life) THEN $job.provider = $provider AND $job.tenant = tenant
        AND $life.recovery_class = 'webhook_wait' ELSE false END
}
ORDER BY received_at, id LIMIT $limit
);
