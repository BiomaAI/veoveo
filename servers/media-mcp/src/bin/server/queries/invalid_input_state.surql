RETURN {
    tasks: (SELECT * FROM task),
    inputs: (SELECT * FROM task_input),
    media_tasks: (SELECT * FROM media_task),
    usage: (SELECT * FROM media_usage),
    jobs: (SELECT * FROM provider_job),
    events: (SELECT * FROM provider_event)
};
