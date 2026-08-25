-- Admit the GeoPackage product format already defined by the Map MCP public
-- contract while preserving the closed format set in the canonical store.

DEFINE FIELD OVERWRITE format ON TABLE map_layer_product TYPE "geojson_seq" | "geoparquet" | "geo_package" | "mvt_bundle";
