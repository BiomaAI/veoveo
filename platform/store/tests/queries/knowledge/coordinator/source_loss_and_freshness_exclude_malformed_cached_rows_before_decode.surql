UPDATE type::table($table) SET observation = {malformed: true}; UPDATE knowledge_member SET valid_until = time::now() - 1s;
