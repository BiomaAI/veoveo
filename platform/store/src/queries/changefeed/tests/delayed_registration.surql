LIVE SELECT * FROM type::table($table);
IF $table = 'observation_cleanup_denied' { UPSERT observation_cleanup_counter:fixture SET registrations += 1; };
SLEEP 2s;
