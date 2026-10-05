UPDATE type::table($table) SET observation.collection = 'fixture.foreign'; UPDATE knowledge_member SET valid_until = time::now() - 1s;
