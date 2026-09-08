BEGIN TRANSACTION;
  LET $new_blob = (SELECT * FROM ONLY $blob) = NONE;
  INSERT IGNORE INTO artifact_blob $blob_content RETURN NONE;
  LET $retained = SELECT * FROM ONLY $blob;
  IF $retained = NONE
  OR $retained.tenant != $blob_content.tenant
  OR $retained.sha256 != $blob_content.sha256
  OR $retained.byte_len != $blob_content.byte_len {
    THROW 'artifact_blob_integrity_conflict';
  };
  IF $new_blob {
    INSERT IGNORE INTO artifact_storage_usage {
      id: $storage_usage, tenant: $blob_content.tenant, committed_bytes: 0,
      reserved_bytes: 0, cleanup_bytes: 0, active_uploads: 0, inflight_bytes: 0, inflight_parts: 0
    } RETURN NONE;
    UPDATE ONLY $storage_usage SET committed_bytes += $blob_content.byte_len RETURN NONE;
  };
  CREATE ONLY $artifact CONTENT $artifact_content RETURN NONE;
  INSERT RELATION INTO artifact_grant $grants RETURN NONE;
  CREATE outbox_event CONTENT $outbox RETURN NONE;
COMMIT TRANSACTION;
