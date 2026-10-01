RETURN (SELECT id, created_at, updated_at, retention_expires_at,
    result.payload.structuredContent AS output
FROM task
WHERE {{FINDING_ADMISSION}}
    AND ($analysis = NONE OR id = $analysis)
    AND ($after_id = NONE OR created_at < $after_created_at
        OR (created_at = $after_created_at AND id < $after_id))
    AND ($prefix = NONE OR string::starts_with(type::string(record::id(id)), $prefix))
ORDER BY created_at DESC, id DESC
LIMIT $limit TIMEOUT 10s).map(|$task| [
    type::string(record::id($task.id)),
    $task.created_at,
    $task.updated_at,
    $task.output,
    $task.retention_expires_at
]);
