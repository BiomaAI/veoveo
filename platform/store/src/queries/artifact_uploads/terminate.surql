BEGIN TRANSACTION;
  LET $upload_state = SELECT * FROM ONLY $upload;
  IF $upload_state = NONE OR $upload_state.state = 'completed' { THROW 'artifact_upload_conflict'; };
  IF $upload_state.state IN ['open', 'finalizing', 'verifying'] {
    IF $owner != NONE AND ($upload_state.lease_owner != $owner OR $upload_state.generation != $generation OR $upload_state.lease_until <= $now) {
      THROW 'artifact_upload_conflict';
    };
    IF $terminal = 'expired' AND $upload_state.expires_at > $now { THROW 'artifact_upload_conflict'; };
    LET $usage = type::record('artifact_storage_usage', record::id($upload_state.tenant));
    UPDATE ONLY $usage SET reserved_bytes -= $upload_state.reserved_bytes,
      cleanup_bytes += $upload_state.reserved_bytes, active_uploads -= 1 RETURN NONE;
    UPDATE ONLY $upload SET state = $terminal, failure = $failure, cleanup_pending = true,
      cleanup_bytes = $upload_state.reserved_bytes, reserved_bytes = 0, updated_at = $now RETURN NONE;
  };
COMMIT TRANSACTION;
