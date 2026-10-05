BEGIN; DELETE audit_record; DELETE audit_block; THROW 'qualification_rollback'; COMMIT;
