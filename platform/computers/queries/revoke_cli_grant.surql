BEGIN TRANSACTION;
LET $current = SELECT * FROM ONLY $grant;
IF $current = NONE OR $current.owner_key != $owner_key OR $current.computer_id != $computer_id { THROW "computer_not_found"; };
IF $admission_expires_at <= time::now() { THROW "computer_authority_expired"; };
IF $current.revoked_at = NONE {
  UPDATE $grant SET revoked_at = time::now();
  CREATE outbox_event CONTENT $event;
};
COMMIT TRANSACTION;
