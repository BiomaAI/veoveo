BEGIN TRANSACTION;
  LET $upload_state = SELECT * FROM ONLY $upload;
  IF $upload_state.state = 'completed' {
    IF $upload_state.verified_sha256 != $blob_content.sha256 OR $upload_state.manifest.byte_len != $blob_content.byte_len { THROW 'artifact_upload_integrity'; };
  } ELSE {
    IF $upload_state.state != 'verifying' OR $upload_state.lease_owner != $owner
      OR $upload_state.generation != $generation OR $upload_state.lease_until <= $now { THROW 'artifact_upload_conflict'; };
    IF $upload_state.expires_at <= $now { THROW 'artifact_upload_expired'; };
    IF !fn::artifact_upload_authority_matches($upload_state) { THROW 'artifact_upload_denied'; };
    IF $upload_state.manifest.byte_len != $blob_content.byte_len
      OR ($upload_state.manifest.sha256 != NONE AND $upload_state.manifest.sha256 != $blob_content.sha256)
      OR $upload_state.artifact != $artifact OR $upload_state.object_key != $blob_content.object_key
      OR $upload_state.tenant != $blob_content.tenant { THROW 'artifact_upload_integrity'; };
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
    LET $duplicate_bytes = IF $retained.object_key != $upload_state.object_key { $blob_content.byte_len } ELSE { 0 };
    UPDATE ONLY $storage_usage SET reserved_bytes -= $upload_state.reserved_bytes,
      cleanup_bytes += $duplicate_bytes, active_uploads -= 1 RETURN NONE;
    UPDATE ONLY $upload SET state = 'completed', verified_sha256 = $blob_content.sha256,
      completed_at = $now, updated_at = $now, reserved_bytes = 0,
      cleanup_bytes = $duplicate_bytes, cleanup_pending = $retained.object_key != $upload_state.object_key RETURN NONE;
    fn::append_audit($audit_rows);
  };
COMMIT TRANSACTION;
