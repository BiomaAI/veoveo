BEGIN TRANSACTION;
  LET $upload_state = SELECT * FROM ONLY $upload;
  IF $upload_state.lease_owner != $owner OR $upload_state.generation != $generation OR $upload_state.lease_until <= $now {
    THROW 'artifact_upload_conflict';
  };
  IF $upload_state.state NOT IN ['completed', 'cancelled', 'expired', 'failed'] { THROW 'artifact_upload_conflict'; };
  LET $busy = SELECT VALUE id FROM artifact_upload_part WHERE upload = $upload AND lease_owner != NONE LIMIT 1;
  LET $retained = SELECT VALUE id FROM artifact_blob WHERE tenant = $upload_state.tenant AND object_key = $upload_state.object_key LIMIT 1;
  IF array::len($busy) > 0 OR array::len($retained) > 0 { THROW 'artifact_upload_conflict'; };
  IF $upload_state.cleanup_pending {
    LET $usage = type::record('artifact_storage_usage', record::id($upload_state.tenant));
    UPDATE ONLY $usage SET cleanup_bytes -= $upload_state.cleanup_bytes RETURN NONE;
    UPDATE ONLY $upload SET cleanup_bytes = 0, cleanup_pending = false, updated_at = $now RETURN NONE;
  };
COMMIT TRANSACTION;
