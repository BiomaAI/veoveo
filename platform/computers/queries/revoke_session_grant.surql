BEGIN TRANSACTION;
LET $current = SELECT * FROM ONLY $grant;
IF $current = NONE OR $current.owner_key != $owner_key { THROW "computer_not_found"; };
IF ($connection != NONE AND $current.connection_id != $connection)
  OR ($connection = NONE AND ($admission_expires_at = NONE OR $admission_expires_at <= time::now())) {
  THROW "computer_authority_expired";
};
IF $current.revoked_at = NONE {
  UPDATE $grant SET revoked_at = time::now();
  CREATE outbox_event CONTENT $event;
};
COMMIT TRANSACTION;
