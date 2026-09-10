BEGIN TRANSACTION;
LET $now = time::now();
LET $current = SELECT * FROM ONLY $execution;
IF $current = NONE OR $current.provider_instance_id != $provider { THROW "computer_not_found"; };
IF $current.binding != $binding OR $current.authority != $authority { THROW "computer_state_conflict"; };
IF $expires <= $now + $required_budget OR $expires > $now + 24h { THROW "computer_invalid_input"; };
IF $current.output_access = $expected_output {
  IF $current.stage != "queued" OR $current.dispatch_id != NONE { THROW "computer_invalid_state"; };
  IF $current.output_access != NONE AND ($previous_expires = NONE OR $previous_expires > $now + $required_budget) { THROW "computer_state_conflict"; };
  UPDATE $execution SET output_access = $sealed, updated_at = $now;
};
COMMIT TRANSACTION;
