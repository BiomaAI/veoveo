    LET $duplicate_bytes = IF $retained.object_key != $upload_state.object_key { $blob_content.byte_len } ELSE { 0 };
    UPDATE ONLY $storage_usage SET reserved_bytes -= $upload_state.reserved_bytes,
      cleanup_bytes += $duplicate_bytes, active_uploads -= 1 RETURN NONE;
    UPDATE ONLY $upload SET state = 'completed', verified_sha256 = $blob_content.sha256,
      completed_at = $now, updated_at = $now, reserved_bytes = 0,
      cleanup_bytes = $duplicate_bytes, cleanup_pending = $retained.object_key != $upload_state.object_key RETURN NONE;
    CREATE ONLY $audit.id CONTENT $audit RETURN NONE;
    CREATE outbox_event CONTENT $audit_outbox RETURN NONE;
  };
COMMIT TRANSACTION;
