BEGIN TRANSACTION;
  LET $current = SELECT * FROM ONLY $capacity;
  IF $current.fingerprint != $fingerprint {
    IF $current.fingerprint != $expected {
      THROW "computer_policy_conflict";
    };
    LET $changed = (UPSERT ONLY $capacity SET
      fingerprint = $fingerprint,
      per_owner = $owner_limit,
      per_tenant = $tenant_limit,
      provider = $provider_limit,
      admissions = admissions ?? 0
      WHERE fingerprint = $expected RETURN AFTER);
    IF $changed = NONE { THROW "computer_policy_conflict"; };
  };
COMMIT TRANSACTION;
