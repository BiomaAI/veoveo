BEGIN TRANSACTION;
  LET $current = SELECT * FROM ONLY $capacity;
  IF $current.fingerprint != $fingerprint {
    IF $current.fingerprint != $expected {
      THROW "computer_policy_conflict";
    };
    UPSERT $capacity SET
      fingerprint = $fingerprint,
      per_owner = $owner_limit,
      per_tenant = $tenant_limit,
      provider = $provider_limit,
      admissions = $current.admissions ?? 0;
  };
COMMIT TRANSACTION;
