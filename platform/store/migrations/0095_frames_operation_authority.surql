-- Every operation records the caller's profile and original optional tenant identity.
DEFINE FIELD authority ON TABLE coordinate_operation TYPE object;
DEFINE FIELD authority.profile ON TABLE coordinate_operation TYPE record<profile>;
DEFINE FIELD authority.tenant_key ON TABLE coordinate_operation TYPE option<string>;
