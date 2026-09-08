BEGIN TRANSACTION;
  LET $upload_state = SELECT * FROM ONLY $upload;
  IF $upload_state = NONE { THROW 'artifact_upload_conflict'; };
  IF $upload_state.policy_digest != $policy_digest OR !fn::artifact_upload_authority_matches($upload_state) { THROW 'artifact_upload_denied'; };
  IF $upload_state.state IN ['finalizing', 'verifying', 'completed'] {
    IF $upload_state.manifest.byte_len != $manifest.byte_len OR $upload_state.manifest.part_count != $manifest.part_count
      OR $upload_state.manifest.sha256 != $manifest.sha256 { THROW 'artifact_upload_conflict'; };
  } ELSE {
    IF $upload_state.state != 'open' OR $upload_state.multipart_id = NONE { THROW 'artifact_upload_conflict'; };
    IF $upload_state.expires_at <= $now { THROW 'artifact_upload_expired'; };
    IF $manifest.byte_len > $upload_state.layout.max_total_bytes OR $manifest.byte_len > $upload_state.reserved_bytes
      OR $manifest.part_count > $upload_state.layout.max_parts
      OR ($upload_state.descriptor.byte_len != NONE AND $upload_state.descriptor.byte_len != $manifest.byte_len)
      OR ($upload_state.descriptor.sha256 != NONE AND $upload_state.descriptor.sha256 != $manifest.sha256)
      OR $upload_state.accepted_bytes != $manifest.byte_len OR $upload_state.accepted_part_count != $manifest.part_count {
      THROW 'artifact_upload_conflict';
    };
    LET $busy = SELECT VALUE id FROM artifact_upload_part WHERE upload = $upload AND lease_owner != NONE LIMIT 1;
    IF array::len($busy) > 0 { THROW 'artifact_upload_busy'; };
    LET $invalid = SELECT VALUE id FROM artifact_upload_part WHERE upload = $upload AND
      (state != 'accepted' OR content_id = NONE OR part_number > $manifest.part_count
        OR (part_number < $manifest.part_count AND byte_len != $upload_state.layout.part_bytes)
        OR (part_number = $manifest.part_count AND byte_len != $manifest.byte_len - (($manifest.part_count - 1) * $upload_state.layout.part_bytes))) LIMIT 1;
    IF array::len($invalid) > 0 { THROW 'artifact_upload_conflict'; };
    UPDATE ONLY $upload SET state = 'finalizing', manifest = $manifest, updated_at = $now RETURN NONE;
  };
COMMIT TRANSACTION;
