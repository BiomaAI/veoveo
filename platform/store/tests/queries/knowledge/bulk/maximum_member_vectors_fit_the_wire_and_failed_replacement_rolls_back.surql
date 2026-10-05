SELECT count() AS total FROM type::table($table) GROUP ALL; SELECT VALUE embedding FROM type::table($table) LIMIT 1;
