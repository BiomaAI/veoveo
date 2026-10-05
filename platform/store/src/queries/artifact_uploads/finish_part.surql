BEGIN TRANSACTION;
  LET $existing = SELECT * FROM ONLY $part;
  IF $existing.lease_owner = $lease_owner AND $existing.generation = $generation {
    LET $upload_state = SELECT * FROM ONLY $existing.upload;
    IF $content_id != NONE {
      IF $upload_state.state != 'open' OR $existing.lease_until <= $now { THROW 'artifact_upload_conflict'; };
      IF $upload_state.expires_at <= $now { THROW 'artifact_upload_expired'; };
      IF $upload_state.policy_digest != $policy_digest OR !fn::artifact_upload_authority_matches($upload_state) { THROW 'artifact_upload_denied'; };
      UPDATE ONLY $part SET state = 'accepted', content_id = $content_id RETURN NONE;
      LET $next_expiry = $now + duration::from_secs($upload_state.inactivity_seconds);
      UPDATE ONLY $upload_state.id SET accepted_bytes += $existing.byte_len, accepted_part_count += 1,
        updated_at = $now, expires_at = IF $next_expiry < lifetime_ends_at { $next_expiry } ELSE { lifetime_ends_at } RETURN NONE;
    };
    LET $usage = type::record('artifact_storage_usage', record::id($existing.tenant));
    UPDATE ONLY $usage SET inflight_bytes -= $existing.byte_len, inflight_parts -= 1 RETURN NONE;
    UPDATE ONLY artifact_upload_memory:global SET inflight_bytes -= $existing.byte_len RETURN NONE;
    UPDATE ONLY $part SET lease_owner = NONE, lease_until = NONE, updated_at = $now RETURN NONE;
  } ELSE IF $content_id != NONE {
    IF $existing.state != 'accepted' OR $existing.content_id != $content_id { THROW 'artifact_upload_conflict'; };
  };
COMMIT TRANSACTION;
