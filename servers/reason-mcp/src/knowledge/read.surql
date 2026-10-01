RETURN (SELECT id, created_at, updated_at,
    result.payload.structuredContent AS output
FROM task
WHERE tenant = $tenant
    AND server = $reason_server
    AND task_type = $reason_task_type
    AND status = 'succeeded'
    AND (retention_expires_at = NONE OR retention_expires_at > time::now())
    AND (result.payload.isError ?? false) = false
    AND ($analysis = NONE OR id = $analysis)
    AND ($after_id = NONE OR created_at < $after_created_at
        OR (created_at = $after_created_at AND id < $after_id))
    AND result.payload.structuredContent.results_artifact.artifact_id IN (
        SELECT VALUE type::string(record::id(id)) FROM artifact_occurrence
        WHERE {{ADMISSION}}
            AND metadata.provenance.kind = 'reason_results'
            AND metadata.provenance.analysis_id = type::string(record::id($parent.id))
    )
ORDER BY created_at DESC, id DESC
LIMIT $limit).map(|$task| [
    type::string(record::id($task.id)),
    $task.created_at,
    $task.updated_at,
    $task.output
]);
