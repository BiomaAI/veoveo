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
