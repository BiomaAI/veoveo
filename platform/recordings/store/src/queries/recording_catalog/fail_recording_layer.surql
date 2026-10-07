BEGIN TRANSACTION;
LET $parent = (SELECT * FROM ONLY $recording FOR UPDATE);
LET $intent = (SELECT * FROM ONLY $publication FOR UPDATE);
IF $parent = NONE OR $parent.tenant != $tenant { THROW 'recording_layer_parent_conflict'; };
LET $current = (SELECT * FROM ONLY $layer FOR UPDATE);
IF $intent != NONE AND NOT ($parent.state = 'sealed' AND $current.kind = 'derived' AND type::string(record::id($layer)) NOT IN $intent.body.layers.layerId) { THROW 'recording_source_selection_frozen'; };
 IF $current.tenant != $tenant OR $current.recording != $recording OR $current.revision != $revision OR $current.state IN ['committed', 'failed'] { THROW 'recording_layer_revision_conflict'; }; UPDATE ONLY $layer SET state = 'failed', failure_reason = $reason, staging_path = NONE, updated_at = time::now(), revision += 1 RETURN NONE; COMMIT TRANSACTION;
