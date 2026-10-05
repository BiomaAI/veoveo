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
      AND server = $server AND tenant = $tenant
    AND owner = $owner AND profile = $profile
    AND (IF type::is_object(request) THEN
        IF type::is_object(request.owner) THEN
            IF type::is_array(request.owner.data_labels) THEN
                request.owner.principal_key = $principal_key
                AND request.owner.profile = $profile_key
                AND (request.owner.tenant_key ?? NONE) = $tenant_key
                AND request.owner.data_labels ALLINSIDE $labels
            ELSE false END
        ELSE false END
    ELSE false END)
 AND task_type IN $task_types
      RETURN AFTER
    );
    IF $task_updated = NONE {
      THROW 'task cannot accept input';
    };
  };
  RETURN $updated;
COMMIT TRANSACTION;
