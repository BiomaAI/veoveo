-- Current public Frames definition trees; native record columns are unchanged.
REMOVE FIELD definition.frames.*.frame_id ON frame_world_revision;
REMOVE FIELD definition.frames.*.parent_frame_id ON frame_world_revision;
REMOVE FIELD definition.frames.*.parent_transform ON frame_world_revision;
DEFINE FIELD OVERWRITE definition.frames.* ON frame_world_revision TYPE object ASSERT object::keys($value) ALLINSIDE ['frameId', 'basis', 'parentFrameId', 'parentTransform', 'description'] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE definition.frames.*.frameId ON frame_world_revision TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE definition.frames.*.basis ON frame_world_revision TYPE { kind: 'ecef_wgs84' | 'enu' | 'ned' | 'frd' | 'optical_rdf' } | { kind: 'cartesian', axes: { x: 'right' | 'left' | 'up' | 'down' | 'forward' | 'back', y: 'right' | 'left' | 'up' | 'down' | 'forward' | 'back', z: 'right' | 'left' | 'up' | 'down' | 'forward' | 'back' } } PERMISSIONS FULL;
DEFINE FIELD OVERWRITE definition.frames.*.parentFrameId ON frame_world_revision TYPE none | null | string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE definition.frames.*.parentTransform ON frame_world_revision TYPE none | null | { kind: 'geodetic_tangent', origin: { latitudeDegrees: number, longitudeDegrees: number, ellipsoidHeightM: number } } | { kind: 'static_rigid', translationM: array<number>, rotationXyzw: array<number> } | { kind: 'dynamic_stream', streamUri: string, entityPath: string } PERMISSIONS FULL;
DEFINE FIELD OVERWRITE definition.frames.*.description ON frame_world_revision TYPE none | null | string PERMISSIONS FULL;
