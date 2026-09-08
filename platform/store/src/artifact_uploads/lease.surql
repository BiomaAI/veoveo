BEGIN TRANSACTION;
  LET $upload_state = SELECT * FROM ONLY $upload;
  IF $upload_state = NONE { THROW 'artifact_upload_conflict'; };
  IF $operation = 'claim' {
    IF $upload_state.lease_owner != NONE AND $upload_state.lease_until > $now { THROW 'artifact_upload_busy'; };
    UPDATE ONLY $upload SET lease_owner = $owner, lease_until = $until,
      generation += 1, updated_at = $now RETURN NONE;
  } ELSE {
    IF $upload_state.lease_owner != $owner OR $upload_state.generation != $generation {
      THROW 'artifact_upload_conflict';
    };
    IF $operation = 'release' {
      UPDATE ONLY $upload SET lease_owner = NONE, lease_until = NONE, updated_at = $now RETURN NONE;
    } ELSE {
      IF $upload_state.lease_until <= $now { THROW 'artifact_upload_conflict'; };
      IF $operation = 'renew' {
        UPDATE ONLY $upload SET lease_until = $until, updated_at = $now RETURN NONE;
      } ELSE {
        IF $upload_state.expires_at <= $now { THROW 'artifact_upload_expired'; };
        IF !fn::artifact_upload_authority_matches($upload_state) { THROW 'artifact_upload_denied'; };
        IF $operation = 'initialize' {
          IF $upload_state.state != 'open' OR ($upload_state.multipart_id != NONE AND $upload_state.multipart_id != $multipart_id) {
            THROW 'artifact_upload_conflict';
          };
          UPDATE ONLY $upload SET multipart_id = $multipart_id, updated_at = $now RETURN NONE;
        } ELSE IF $operation = 'verify' {
          IF $upload_state.state NOT IN ['finalizing', 'verifying'] { THROW 'artifact_upload_conflict'; };
          UPDATE ONLY $upload SET state = 'verifying', updated_at = $now RETURN NONE;
        } ELSE { THROW 'artifact_upload_conflict'; };
      };
    };
  };
COMMIT TRANSACTION;
