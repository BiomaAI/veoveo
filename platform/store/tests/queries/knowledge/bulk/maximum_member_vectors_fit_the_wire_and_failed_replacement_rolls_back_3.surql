SELECT count() AS total FROM type::table($table) WHERE title = $title GROUP ALL;
