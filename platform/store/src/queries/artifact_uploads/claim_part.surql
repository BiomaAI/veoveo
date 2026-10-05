BEGIN TRANSACTION;
  LET $upload_state = SELECT * FROM ONLY $upload;
  IF $upload_state = NONE OR $upload_state.state != 'open' { THROW 'artifact_upload_conflict'; };
  IF $upload_state.expires_at <= $now { THROW 'artifact_upload_expired'; };
  IF $upload_state.policy_digest != $policy_digest OR !fn::artifact_upload_authority_matches($upload_state) {
    THROW 'artifact_upload_denied';
  };
  LET $offset = ($part_number - 1) * $upload_state.layout.part_bytes;
  IF $part_number > $upload_state.layout.max_parts OR $byte_len > $upload_state.layout.part_bytes
    OR $offset >= $upload_state.layout.max_total_bytes OR ($byte_len = 0 AND $part_number != 1)
    OR $offset + $byte_len > $upload_state.layout.max_total_bytes {
    THROW 'artifact_upload_conflict';
  };
  IF $upload_state.descriptor.byte_len != NONE {
    LET $remaining = $upload_state.descriptor.byte_len - $offset;
    LET $expected = IF $remaining < $upload_state.layout.part_bytes { $remaining } ELSE { $upload_state.layout.part_bytes };
    IF $remaining < 0 OR $byte_len != $expected { THROW 'artifact_upload_conflict'; };
  };
  LET $existing = SELECT * FROM ONLY $part;
  IF $existing != NONE AND ($existing.byte_len != $byte_len OR $existing.sha256 != $sha256) {
    THROW 'artifact_upload_conflict';
  };
  IF $existing = NONE OR $existing.state != 'accepted' {
    IF $existing.lease_owner != NONE AND $existing.lease_until > $now { THROW 'artifact_upload_busy'; };
    LET $usage = type::record('artifact_storage_usage', record::id($upload_state.tenant));
    IF $existing.lease_owner != NONE {
      UPDATE ONLY $usage SET inflight_bytes -= $existing.byte_len, inflight_parts -= 1 RETURN NONE;
      UPDATE ONLY artifact_upload_memory:global SET inflight_bytes -= $existing.byte_len RETURN NONE;
    };
    LET $used = SELECT * FROM ONLY $usage;
    LET $memory = SELECT * FROM ONLY artifact_upload_memory:global;
    LET $active_parts = (SELECT VALUE id FROM artifact_upload_part
      WHERE upload = $upload AND lease_owner != NONE AND lease_until > $now) ?? [];
    IF $used.inflight_parts >= $tenant_parts OR array::len($active_parts) >= $upload_state.layout.parallel_parts
      OR $memory.inflight_bytes + $byte_len > $max_inflight_bytes { THROW 'artifact_upload_busy'; };
    IF $existing = NONE AND $upload_state.descriptor.byte_len = NONE {
      LET $pending = (SELECT VALUE math::sum(byte_len) FROM artifact_upload_part WHERE upload = $upload AND state = 'reserved' GROUP ALL)[0] ?? 0;
      LET $required = $upload_state.accepted_bytes + $pending + $byte_len;
      IF $required > $upload_state.reserved_bytes {
        LET $window = $upload_state.layout.part_bytes * $upload_state.layout.parallel_parts;
        LET $remaining = $upload_state.layout.max_total_bytes - $upload_state.reserved_bytes;
        LET $additional = IF $window < $remaining { $window } ELSE { $remaining };
        IF $upload_state.reserved_bytes + $additional < $required
          OR $used.committed_bytes + $used.reserved_bytes + $used.cleanup_bytes + $additional > $quota { THROW 'artifact_upload_quota'; };
        UPDATE ONLY $usage SET reserved_bytes += $additional RETURN NONE;
        UPDATE ONLY $upload SET reserved_bytes += $additional RETURN NONE;
      };
    };
    IF $existing = NONE {
      CREATE ONLY $part CONTENT {
        upload: $upload, tenant: $upload_state.tenant, part_number: $part_number, byte_len: $byte_len,
        sha256: $sha256, state: 'reserved', generation: 1, lease_owner: $lease_owner,
        lease_until: $lease_until, created_at: $now, updated_at: $now
      } RETURN NONE;
    } ELSE {
      UPDATE ONLY $part SET generation += 1, lease_owner = $lease_owner, lease_until = $lease_until, updated_at = $now RETURN NONE;
    };
    UPDATE ONLY $usage SET inflight_bytes += $byte_len, inflight_parts += 1 RETURN NONE;
    UPDATE ONLY artifact_upload_memory:global SET inflight_bytes += $byte_len RETURN NONE;
    LET $next_expiry = $now + duration::from_secs($upload_state.inactivity_seconds);
    UPDATE ONLY $upload SET updated_at = $now,
      expires_at = IF $next_expiry < lifetime_ends_at { $next_expiry } ELSE { lifetime_ends_at } RETURN NONE;
  };
COMMIT TRANSACTION;
