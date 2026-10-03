BEGIN TRANSACTION;
  LET $updated = (
    UPDATE ONLY $input SET
      response = $response,
      responded_at = $now
    WHERE
      task = $task
      AND request_key = $key
      AND response = NONE
    RETURN AFTER
  );
  IF $updated != NONE {
    LET $task_updated = (
      UPDATE ONLY $task SET
        updated_at = $now
      WHERE
        server = $server
        AND status IN ['queued', 'running', 'waiting']
      /* caller selection */
      RETURN AFTER
    );
    IF $task_updated = NONE {
      THROW 'task cannot accept input';
    };
  };
  RETURN $updated;
COMMIT TRANSACTION;
