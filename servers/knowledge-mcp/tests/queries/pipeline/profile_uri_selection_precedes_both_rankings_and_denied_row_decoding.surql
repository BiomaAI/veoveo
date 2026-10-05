UPDATE type::table($table) SET observation.collection = 'fixture.foreign' WHERE string::contains(uri, 'aaa-denied') OR string::ends_with(uri, 'x-end-end');
