UPDATE type::table($table) SET observation = {malformed: true} WHERE admission.labels CONTAINS 'secret';
