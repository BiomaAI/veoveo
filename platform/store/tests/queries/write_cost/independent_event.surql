BEGIN TRANSACTION; UPDATE ONLY $state SET revision += 1 RETURN NONE;

CREATE ONLY $event SET sequence = $sequence,
    aggregate_type = 'measurement_state', aggregate_id = $aggregate,
    event_type = 'measurement.updated', schema_version = 1,
    payload = { revision: $revision } RETURN NONE;

 COMMIT TRANSACTION;
