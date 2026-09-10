BEGIN TRANSACTION;
LET $now = time::now();
LET $current = SELECT * FROM ONLY $grant;
LET $limits = SELECT * FROM ONLY $policy;
IF $current = NONE OR $limits = NONE OR $limits.fingerprint != $policy_fingerprint
  OR $limits.max_grants = 0 OR $current.connection_id != $connection OR $current.revoked_at != NONE
  OR $current.expires_at <= $now OR $current.idle_expires_at <= $now OR $previous_idle_end <= $now OR $absolute_end <= $now {
  THROW "computer_authority_expired";
};
LET $family = SELECT * FROM ONLY $current.family;
IF $family = NONE OR $family.revoked_at != NONE OR $family.expires_at <= $now { THROW "computer_authority_expired"; };
LET $end = IF $now + $idle < $absolute_end { $now + $idle } ELSE { $absolute_end };
UPDATE $grant SET last_activity_at = $now, idle_expires_at = $end;
COMMIT TRANSACTION;
