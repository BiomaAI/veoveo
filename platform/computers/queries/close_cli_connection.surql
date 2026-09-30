BEGIN TRANSACTION;
LET $current = SELECT * FROM ONLY $connection FOR UPDATE;
IF $current != NONE AND $current.connection_id = $connection_id
  AND $current.grant_id = $grant_id AND $current.closed_at = NONE {
  UPDATE ONLY $connection SET closed_at = time::now();
  fn::append_audit($audit_rows);
};
COMMIT TRANSACTION;
