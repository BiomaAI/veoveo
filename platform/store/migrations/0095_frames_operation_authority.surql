-- Existing operations retain their original contents and have no inferred authority.
-- Frames admits version 1 records only. Installation owners resolve retained v0 data
-- during the coordinated upgrade described in servers/frames-mcp/DESIGN.md.
DEFINE FIELD authority ON TABLE coordinate_operation TYPE option<object>;
DEFINE FIELD authority.version ON TABLE coordinate_operation TYPE option<int> ASSERT $value = NONE OR $value = 1;
DEFINE FIELD authority.profile ON TABLE coordinate_operation TYPE option<record<profile>>;
DEFINE FIELD authority.tenant_key ON TABLE coordinate_operation TYPE option<string>;
