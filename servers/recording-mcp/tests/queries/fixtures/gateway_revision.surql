CREATE ONLY $record CONTENT $content RETURN NONE;
CREATE ONLY gateway_control_active:current CONTENT {
    revision: $record,
    revision_id: $revision_id,
    updated_at: time::now()
} RETURN NONE;
