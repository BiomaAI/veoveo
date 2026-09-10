BEGIN TRANSACTION;
LET $now = time::now();
LET $current = SELECT * FROM ONLY $grant;
LET $limits = SELECT * FROM ONLY $policy;
IF $current = NONE OR $limits = NONE OR $limits.fingerprint != $policy_fingerprint
  OR $limits.max_grants = 0 OR $current.revoked_at != NONE OR $current.expires_at <= $now
  OR $current.idle_expires_at <= $now OR $previous_idle_end <= $now OR $absolute_end <= $now
  OR $lease_end <= $now { THROW "computer_authority_expired"; };
LET $family = SELECT * FROM ONLY $current.family;
IF $family = NONE OR $family.revoked_at != NONE OR $family.expires_at <= $now { THROW "computer_authority_expired"; };
LET $renewed = (UPDATE ONLY $connection SET expires_at = $lease_end
  WHERE grant_id = $grant_id AND connection_id = $connection_id
  AND closed_at = NONE AND expires_at > $now RETURN AFTER);
IF $renewed = NONE { THROW "computer_authority_expired"; };
IF $activity {
  LET $end = IF $now + $idle < $absolute_end { $now + $idle } ELSE { $absolute_end };
  UPDATE $grant SET last_activity_at = $now, idle_expires_at = $end;
};
COMMIT TRANSACTION;
