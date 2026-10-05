-- Current owner schema; schema definitions only.

DEFINE TABLE coordinate_operation TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE frame_world TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE frame_world_revision TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;


DEFINE FIELD OVERWRITE authority ON coordinate_operation TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.profile ON coordinate_operation TYPE record<profile> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.tenant_key ON coordinate_operation TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE classification ON coordinate_operation TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON coordinate_operation TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON coordinate_operation TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE kind ON coordinate_operation TYPE 'frame_conversion' | 'crs_transform' | 'local_frame_derivation' | 'geodesic_inverse' | 'geodesic_direct' | 'geofence_validation' | 'batch' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE labels ON coordinate_operation TYPE array<string> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE labels.* ON coordinate_operation TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE operation_key ON coordinate_operation TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON coordinate_operation TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provenance ON coordinate_operation TYPE object ASSERT object::keys($value) ALLINSIDE ['operation', 'kind', 'source_crs', 'target_crs', 'engine', 'grid_packages', 'approximation_used', 'accuracy_m', 'warnings'] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE provenance.operation ON coordinate_operation TYPE object ASSERT object::keys($value) ALLINSIDE ['operation_id', 'operation_uri', 'source_frame', 'target_frame', 'created_at'] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE provenance.operation.operation_id ON coordinate_operation TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE provenance.operation.operation_uri ON coordinate_operation TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE provenance.operation.created_at ON coordinate_operation TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE provenance.operation.source_frame ON coordinate_operation TYPE none | null | { kind: 'wgs84' | 'ecef_wgs84' } | { kind: 'world_frame', frame_uri: string } PERMISSIONS FULL;
DEFINE FIELD OVERWRITE provenance.operation.target_frame ON coordinate_operation TYPE none | null | { kind: 'wgs84' | 'ecef_wgs84' } | { kind: 'world_frame', frame_uri: string } PERMISSIONS FULL;
DEFINE FIELD OVERWRITE provenance.kind ON coordinate_operation TYPE 'frame_conversion' | 'crs_transform' | 'local_frame_derivation' | 'geodesic_inverse' | 'geodesic_direct' | 'geofence_validation' | 'batch' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE provenance.source_crs ON coordinate_operation TYPE none | null | string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE provenance.target_crs ON coordinate_operation TYPE none | null | string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE provenance.engine ON coordinate_operation TYPE none | null | string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE provenance.grid_packages ON coordinate_operation TYPE none | array<string> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE provenance.approximation_used ON coordinate_operation TYPE bool DEFAULT false PERMISSIONS FULL;
DEFINE FIELD OVERWRITE provenance.accuracy_m ON coordinate_operation TYPE none | null | number PERMISSIONS FULL;
DEFINE FIELD OVERWRITE provenance.warnings ON coordinate_operation TYPE none | array<string> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE task ON coordinate_operation TYPE none | record<task> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON coordinate_operation TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON coordinate_operation TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE INDEX coordinate_operation_owner_time ON coordinate_operation FIELDS tenant, owner, created_at;

DEFINE INDEX coordinate_operation_task ON coordinate_operation FIELDS task;

DEFINE INDEX coordinate_operation_tenant_key_unique ON coordinate_operation FIELDS tenant, operation_key UNIQUE;

DEFINE FIELD OVERWRITE classification ON frame_world TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON frame_world TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE description ON frame_world TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE display_name ON frame_world TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE head_revision ON frame_world TYPE none | record<frame_world_revision> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE head_revision_key ON frame_world TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON frame_world TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE labels ON frame_world TYPE array<string> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE labels.* ON frame_world TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON frame_world TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revision ON frame_world TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON frame_world TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON frame_world TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE world_key ON frame_world TYPE string PERMISSIONS FULL;

DEFINE INDEX frame_world_owner ON frame_world FIELDS tenant, owner;

DEFINE INDEX frame_world_tenant_key_unique ON frame_world FIELDS tenant, world_key UNIQUE;

DEFINE FIELD OVERWRITE created_at ON frame_world_revision TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE frame_ids ON frame_world_revision TYPE array<string> ASSERT array::len($value) > 0;
DEFINE FIELD OVERWRITE definition ON frame_world_revision TYPE object ASSERT object::keys($value) ALLINSIDE ['frames'] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE definition.frames ON frame_world_revision TYPE array<object> ASSERT array::len($value) > 0 AND array::len($value) <= 10000 PERMISSIONS FULL;
DEFINE FIELD OVERWRITE definition.frames.* ON frame_world_revision TYPE object ASSERT object::keys($value) ALLINSIDE ['frame_id', 'basis', 'parent_frame_id', 'parent_transform', 'description'] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE definition.frames.*.frame_id ON frame_world_revision TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE definition.frames.*.basis ON frame_world_revision TYPE { kind: 'ecef_wgs84' | 'enu' | 'ned' | 'frd' | 'optical_rdf' } | { kind: 'cartesian', axes: { x: 'right' | 'left' | 'up' | 'down' | 'forward' | 'back', y: 'right' | 'left' | 'up' | 'down' | 'forward' | 'back', z: 'right' | 'left' | 'up' | 'down' | 'forward' | 'back' } } PERMISSIONS FULL;
DEFINE FIELD OVERWRITE definition.frames.*.parent_frame_id ON frame_world_revision TYPE none | null | string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE definition.frames.*.parent_transform ON frame_world_revision TYPE none | null | { kind: 'geodetic_tangent', origin: { latitude_degrees: number, longitude_degrees: number, ellipsoid_height_m: number } } | { kind: 'static_rigid', translation_m: array<number>, rotation_xyzw: array<number> } | { kind: 'dynamic_stream', stream_uri: string, entity_path: string } PERMISSIONS FULL;
DEFINE FIELD OVERWRITE definition.frames.*.description ON frame_world_revision TYPE none | null | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON frame_world_revision TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON frame_world_revision TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revision ON frame_world_revision TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revision_key ON frame_world_revision TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE root_frame_key ON frame_world_revision TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE spec_sha256 ON frame_world_revision TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON frame_world_revision TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE world ON frame_world_revision TYPE record<frame_world> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE world_key ON frame_world_revision TYPE string PERMISSIONS FULL;

DEFINE INDEX frame_world_revision_digest ON frame_world_revision FIELDS world, spec_sha256;

DEFINE INDEX frame_world_revision_key_unique ON frame_world_revision FIELDS tenant, world_key, revision_key UNIQUE;

DEFINE INDEX frame_world_revision_number_unique ON frame_world_revision FIELDS world, revision UNIQUE;
