BEGIN; DELETE $parent; THROW 'qualification_rollback'; COMMIT;
