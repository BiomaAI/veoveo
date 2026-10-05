BEGIN TRANSACTION; CREATE ONLY $revision CONTENT $content; UPSERT gateway_control_active:current SET revision=$revision, revision_id=$id, updated_at=time::now(); COMMIT TRANSACTION;
