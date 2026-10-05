LIVE SELECT * FROM type::table($table);
IF $table = 'observation_cleanup_denied' { THROW 'fixture registration denial'; };
