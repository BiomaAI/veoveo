tenant = $tenant
    AND server = $reason_server
    AND task_type = $reason_task_type
    AND status = 'succeeded'
    AND (retention_expires_at = NONE OR retention_expires_at > time::now())
    AND (result.payload.isError ?? false) = false
    AND result.payload.structuredContent.results_artifact.artifact_id IN (
        SELECT VALUE type::string(record::id(id)) FROM artifact_occurrence
        WHERE {{ADMISSION}}
            AND metadata.provenance.kind = 'reason_results'
            AND metadata.provenance.analysis_id = type::string(record::id($parent.id))
    )
