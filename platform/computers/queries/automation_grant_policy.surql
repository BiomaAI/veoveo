BEGIN TRANSACTION;
LET $current = SELECT * FROM ONLY $policy;
IF $current = NONE {
  IF $expected != NONE { THROW "computer_policy_conflict"; };
  CREATE $policy CONTENT {fingerprint:$fingerprint, max_grants:$max_grants,
    maximum_lifetime_seconds:$maximum_lifetime_seconds,
    maximum_execution_seconds:$maximum_execution_seconds,
    maximum_output_bytes:$maximum_output_bytes, admissions:0};
} ELSE IF $current.fingerprint != $fingerprint {
  IF $expected = NONE OR $current.fingerprint != $expected { THROW "computer_policy_conflict"; };
  UPDATE $policy SET fingerprint=$fingerprint, max_grants=$max_grants,
    maximum_lifetime_seconds=$maximum_lifetime_seconds,
    maximum_execution_seconds=$maximum_execution_seconds, maximum_output_bytes=$maximum_output_bytes;
};
COMMIT TRANSACTION;
