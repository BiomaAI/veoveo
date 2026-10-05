UPDATE type::table($table) SET observation = {malformed: true} WHERE string::contains(uri, 'aaa-denied') OR string::ends_with(uri, 'x-end-end');
