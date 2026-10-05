UPDATE type::table($table) SET observation = {malformed: true} WHERE admission.tenant_read = false AND admission.context_read = false;
