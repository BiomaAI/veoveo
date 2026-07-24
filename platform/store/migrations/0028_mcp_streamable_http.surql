-- Remove registrations whose transport cannot satisfy the revision-2 MCP
-- contract, then constrain future registrations to Streamable HTTP.

DELETE mcp_server WHERE transport != "streamable_http";
DEFINE FIELD OVERWRITE transport ON TABLE mcp_server TYPE "streamable_http";
