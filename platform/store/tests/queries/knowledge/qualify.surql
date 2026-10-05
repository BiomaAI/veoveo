UPDATE type::table($table) SET observation.collection = 'fixture.foreign' WHERE admission.labels CONTAINS 'secret';
