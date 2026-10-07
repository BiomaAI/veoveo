BEGIN TRANSACTION;
LET $parent_id = $content.recording;
LET $parent = (SELECT * FROM ONLY $parent_id FOR UPDATE);
LET $intent = (SELECT * FROM ONLY $publication FOR UPDATE);
IF $parent = NONE OR $parent.tenant != $content.tenant { THROW 'recording_blueprint_parent_conflict'; };
IF $intent != NONE { THROW 'recording_source_selection_frozen'; };
CREATE ONLY $blueprint CONTENT $content RETURN NONE;
UPDATE ONLY $content.recording SET revision += 1, updated_at = time::now() RETURN NONE;
COMMIT TRANSACTION;
