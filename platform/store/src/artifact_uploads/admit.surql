BEGIN TRANSACTION;
  LET $existing = SELECT * FROM artifact_upload WHERE tenant = $content.tenant
    AND actor = $content.actor AND profile_key = $content.profile_key
    AND work_context = $content.work_context AND request_id = $content.request_id;
  IF array::len($existing) > 0 {
    IF $existing[0].descriptor.filename != $content.descriptor.filename
      OR $existing[0].descriptor.mime_type != $content.descriptor.mime_type
      OR $existing[0].descriptor.byte_len != $content.descriptor.byte_len
      OR $existing[0].descriptor.sha256 != $content.descriptor.sha256 {
      THROW 'artifact_upload_conflict';
    };
  } ELSE {
    IF !fn::artifact_upload_authority_matches($content) { THROW 'artifact_upload_denied'; };
    INSERT IGNORE INTO artifact_storage_usage {
      id: $usage, tenant: $content.tenant, committed_bytes: 0, reserved_bytes: 0,
      cleanup_bytes: 0, active_uploads: 0, inflight_bytes: 0, inflight_parts: 0
    } RETURN NONE;
    LET $used = SELECT * FROM ONLY $usage;
    IF $used.active_uploads >= $active_limit { THROW 'artifact_upload_busy'; };
    IF $used.committed_bytes + $used.reserved_bytes + $used.cleanup_bytes + $content.reserved_bytes > $quota {
      THROW 'artifact_upload_quota';
    };
    UPDATE ONLY $usage SET reserved_bytes += $content.reserved_bytes, active_uploads += 1 RETURN NONE;
    CREATE ONLY $content.id CONTENT $content RETURN NONE;
  };
COMMIT TRANSACTION;
