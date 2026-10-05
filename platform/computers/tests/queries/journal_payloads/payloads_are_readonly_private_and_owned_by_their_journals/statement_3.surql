BEGIN; DELETE $journal; THROW 'qualification_rollback'; COMMIT;
