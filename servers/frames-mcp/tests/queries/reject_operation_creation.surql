DEFINE EVENT reject_operation_event ON TABLE coordinate_operation WHEN $event = 'CREATE' THEN { THROW 'fixture rejected operation commit'; };
