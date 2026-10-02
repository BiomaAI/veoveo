RETURN {
  LET $exists = SELECT VALUE id FROM ONLY $journal;
  LET $admitted = SELECT *, payload.sealed AS sealed FROM ONLY $journal
  WHERE id != NONE AND provider_instance_id = $provider
    AND binding = $binding AND authority = $authority;
  IF $exists != NONE AND $admitted = NONE { THROW "computer_state_conflict"; };
  $admitted;
};
