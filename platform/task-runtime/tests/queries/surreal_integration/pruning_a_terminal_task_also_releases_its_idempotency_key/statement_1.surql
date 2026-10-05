BEGIN; DELETE $task; THROW 'qualification_rollback'; COMMIT;
