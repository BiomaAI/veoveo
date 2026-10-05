SELECT VALUE source_key FROM map_source WHERE tenant = $tenant
             AND string::lowercase(source_key) CONTAINS $needle
             GROUP BY source_key ORDER BY source_key ASC LIMIT 101 TIMEOUT 5s;
