BEGIN TRANSACTION; CREATE audit_target_fixture:rollback SET revision=1; fn::append_audit($audit_rows); COMMIT TRANSACTION;
