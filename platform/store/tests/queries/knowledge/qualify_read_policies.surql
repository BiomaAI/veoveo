UPDATE type::table($table) SET observation.collection = 'fixture.foreign' WHERE admission.tenant_read = false AND admission.context_read = false;
