DEFINE EVENT reject_fixture ON TABLE $table WHEN $event = 'CREATE' AND $after.ordinal = 128 THEN { THROW 'fixture_replacement_rejected'; };
