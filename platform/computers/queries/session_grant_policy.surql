BEGIN TRANSACTION;
LET $current = SELECT * FROM ONLY $policy;
IF $current = NONE {
  IF $expected != NONE { THROW "computer_policy_conflict"; };
  CREATE $policy CONTENT { fingerprint: $fingerprint, max_grants: $max_grants,
    absolute_seconds: $absolute_seconds, idle_seconds: $idle_seconds, admissions: 0 };
} ELSE IF $current.fingerprint != $fingerprint {
  IF $expected = NONE OR $current.fingerprint != $expected { THROW "computer_policy_conflict"; };
  UPDATE $policy SET fingerprint = $fingerprint, max_grants = $max_grants,
    absolute_seconds = $absolute_seconds, idle_seconds = $idle_seconds;
};
COMMIT TRANSACTION;
