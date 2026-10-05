-- Current owner schema and fresh bootstrap state.

DEFINE TABLE map_acquisition TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE map_active_release TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE map_composition TYPE NORMAL SCHEMAFULL CHANGEFEED 12w6d PERMISSIONS NONE;

DEFINE TABLE map_composition_revision TYPE NORMAL SCHEMAFULL CHANGEFEED 12w6d PERMISSIONS NONE;

DEFINE TABLE map_dataset_release TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE map_derivation TYPE NORMAL SCHEMAFULL CHANGEFEED 12w6d PERMISSIONS NONE;

DEFINE TABLE map_feature_changeset TYPE NORMAL SCHEMAFULL CHANGEFEED 12w6d PERMISSIONS NONE;

DEFINE TABLE map_feature_head TYPE NORMAL SCHEMAFULL CHANGEFEED 12w6d PERMISSIONS NONE;

DEFINE TABLE map_feature_layer TYPE NORMAL SCHEMAFULL CHANGEFEED 12w6d PERMISSIONS NONE;

DEFINE TABLE map_feature_revision TYPE NORMAL SCHEMAFULL CHANGEFEED 12w6d PERMISSIONS NONE;

DEFINE TABLE map_feature_schema_revision TYPE NORMAL SCHEMAFULL CHANGEFEED 12w6d PERMISSIONS NONE;

DEFINE TABLE map_layer_product TYPE NORMAL SCHEMAFULL CHANGEFEED 12w6d PERMISSIONS NONE;

DEFINE TABLE map_layer_publication TYPE NORMAL SCHEMAFULL CHANGEFEED 12w6d PERMISSIONS NONE;

DEFINE TABLE map_mobility_profile TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE map_operational_snapshot TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE map_projection_state TYPE NORMAL SCHEMAFULL CHANGEFEED 12w6d PERMISSIONS NONE;

DEFINE TABLE map_restriction TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE map_route TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE map_route_dependency TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE map_route_matrix TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE map_source TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE map_style_revision TYPE NORMAL SCHEMAFULL CHANGEFEED 12w6d PERMISSIONS NONE;

DEFINE FIELD OVERWRITE acquisition_key ON map_acquisition TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE canonical_json ON map_acquisition TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON map_acquisition TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON map_acquisition TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE idempotency_key ON map_acquisition TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON map_acquisition TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE phase ON map_acquisition TYPE 'queued' | 'downloading' | 'verifying' | 'normalizing' | 'building_graph' | 'validating' | 'publishing_artifacts' | 'staging_release' | 'complete' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE record_version ON map_acquisition TYPE int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE source_key ON map_acquisition TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE staged_release_key ON map_acquisition TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE status ON map_acquisition TYPE 'queued' | 'running' | 'succeeded' | 'failed' | 'cancel_requested' | 'cancelled' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON map_acquisition TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON map_acquisition TYPE datetime PERMISSIONS FULL;

DEFINE INDEX map_acquisition_idempotency_unique ON map_acquisition FIELDS tenant, owner, idempotency_key UNIQUE;

DEFINE INDEX map_acquisition_source_time ON map_acquisition FIELDS tenant, source_key, created_at;

DEFINE INDEX map_acquisition_tenant_key_unique ON map_acquisition FIELDS tenant, acquisition_key UNIQUE;

DEFINE FIELD OVERWRITE activated_at ON map_active_release TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE activated_by ON map_active_release TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE dataset_key ON map_active_release TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON map_active_release TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE previous_release_key ON map_active_release TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE record_version ON map_active_release TYPE int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE release_key ON map_active_release TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON map_active_release TYPE record<tenant> PERMISSIONS FULL;

DEFINE INDEX map_active_release_tenant_dataset_unique ON map_active_release FIELDS tenant, dataset_key UNIQUE;

DEFINE FIELD OVERWRITE archived_at ON map_composition TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority ON map_composition TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.classification ON map_composition TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.context_key ON map_composition TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.data_labels ON map_composition TYPE array<string> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.data_labels.* ON map_composition TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.delegation_id ON map_composition TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants ON map_composition TYPE array<object> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.* ON map_composition TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.permission ON map_composition TYPE 'read' | 'write' | 'admin' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.subject_key ON map_composition TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.subject_kind ON map_composition TYPE 'principal' | 'group' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initiator_key ON map_composition TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.invocation_mode ON map_composition TYPE 'direct' | 'delegated' | 'automated' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.membership ON map_composition TYPE 'viewer' | 'contributor' | 'custodian' | 'owner' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.owner_key ON map_composition TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.owner_kind ON map_composition TYPE 'principal' | 'group' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.policy_revision ON map_composition TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE canonical_json ON map_composition TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE composition_key ON map_composition TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON map_composition TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_by_key ON map_composition TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE current_revision ON map_composition TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON map_composition TYPE array PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON map_composition TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner_key ON map_composition TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner_kind ON map_composition TYPE 'principal' | 'group' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON map_composition TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE title ON map_composition TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON map_composition TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON map_composition TYPE record<work_context> PERMISSIONS FULL;

DEFINE INDEX map_composition_context_updated ON map_composition FIELDS tenant, work_context, archived_at, updated_at;

DEFINE INDEX map_composition_key_unique ON map_composition FIELDS tenant, composition_key UNIQUE;

DEFINE FIELD OVERWRITE authority ON map_composition_revision TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.classification ON map_composition_revision TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.context_key ON map_composition_revision TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.data_labels ON map_composition_revision TYPE array<string> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.data_labels.* ON map_composition_revision TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.delegation_id ON map_composition_revision TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants ON map_composition_revision TYPE array<object> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.* ON map_composition_revision TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.permission ON map_composition_revision TYPE 'read' | 'write' | 'admin' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.subject_key ON map_composition_revision TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.subject_kind ON map_composition_revision TYPE 'principal' | 'group' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initiator_key ON map_composition_revision TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.invocation_mode ON map_composition_revision TYPE 'direct' | 'delegated' | 'automated' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.membership ON map_composition_revision TYPE 'viewer' | 'contributor' | 'custodian' | 'owner' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.owner_key ON map_composition_revision TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.owner_kind ON map_composition_revision TYPE 'principal' | 'group' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.policy_revision ON map_composition_revision TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE canonical_json ON map_composition_revision TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE composition_key ON map_composition_revision TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE composition_revision_key ON map_composition_revision TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON map_composition_revision TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_by ON map_composition_revision TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON map_composition_revision TYPE array PERMISSIONS FULL;

DEFINE FIELD OVERWRITE publication_keys ON map_composition_revision TYPE array<string> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE publication_keys.* ON map_composition_revision TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revision ON map_composition_revision TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON map_composition_revision TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON map_composition_revision TYPE record<work_context> PERMISSIONS FULL;

DEFINE INDEX map_composition_revision_key_unique ON map_composition_revision FIELDS tenant, composition_revision_key UNIQUE;

DEFINE INDEX map_composition_revision_unique ON map_composition_revision FIELDS tenant, composition_key, revision UNIQUE;

DEFINE FIELD OVERWRITE canonical_json ON map_dataset_release TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON map_dataset_release TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE dataset_key ON map_dataset_release TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON map_dataset_release TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE record_version ON map_dataset_release TYPE int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE release_key ON map_dataset_release TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE source_digest_sha256 ON map_dataset_release TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE source_key ON map_dataset_release TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE state ON map_dataset_release TYPE 'staged' | 'active' | 'retired' | 'quarantined' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON map_dataset_release TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON map_dataset_release TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE valid_from ON map_dataset_release TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE valid_until ON map_dataset_release TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE version_label ON map_dataset_release TYPE string PERMISSIONS FULL;

DEFINE INDEX map_release_dataset_state ON map_dataset_release FIELDS tenant, dataset_key, state;

DEFINE INDEX map_release_source_time ON map_dataset_release FIELDS tenant, source_key, created_at;

DEFINE INDEX map_release_tenant_key_unique ON map_dataset_release FIELDS tenant, release_key UNIQUE;

DEFINE FIELD OVERWRITE canonical_json ON map_derivation TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON map_derivation TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_by ON map_derivation TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE derivation_key ON map_derivation TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON map_derivation TYPE array PERMISSIONS FULL;

DEFINE FIELD OVERWRITE kind ON map_derivation TYPE 'raster' | 'spatial' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON map_derivation TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON map_derivation TYPE record<work_context> PERMISSIONS FULL;

DEFINE INDEX map_derivation_scope_key ON map_derivation FIELDS tenant, work_context, kind, derivation_key UNIQUE;

DEFINE FIELD OVERWRITE actor_key ON map_feature_changeset TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority ON map_feature_changeset TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.classification ON map_feature_changeset TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.context_key ON map_feature_changeset TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.data_labels ON map_feature_changeset TYPE array<string> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.data_labels.* ON map_feature_changeset TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.delegation_id ON map_feature_changeset TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants ON map_feature_changeset TYPE array<object> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.* ON map_feature_changeset TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.permission ON map_feature_changeset TYPE 'read' | 'write' | 'admin' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.subject_key ON map_feature_changeset TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.subject_kind ON map_feature_changeset TYPE 'principal' | 'group' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initiator_key ON map_feature_changeset TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.invocation_mode ON map_feature_changeset TYPE 'direct' | 'delegated' | 'automated' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.membership ON map_feature_changeset TYPE 'viewer' | 'contributor' | 'custodian' | 'owner' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.owner_key ON map_feature_changeset TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.owner_kind ON map_feature_changeset TYPE 'principal' | 'group' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.policy_revision ON map_feature_changeset TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE base_layer_revision ON map_feature_changeset TYPE int ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE canonical_json ON map_feature_changeset TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE changeset_key ON map_feature_changeset TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE commit_sequence ON map_feature_changeset TYPE int ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON map_feature_changeset TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE feature_keys ON map_feature_changeset TYPE array<string> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE feature_keys.* ON map_feature_changeset TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON map_feature_changeset TYPE array PERMISSIONS FULL;

DEFINE FIELD OVERWRITE idempotency_key ON map_feature_changeset TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE layer_key ON map_feature_changeset TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON map_feature_changeset TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE request_digest_sha256 ON map_feature_changeset TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE resulting_layer_revision ON map_feature_changeset TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON map_feature_changeset TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON map_feature_changeset TYPE record<work_context> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context_key ON map_feature_changeset TYPE string PERMISSIONS FULL;

DEFINE INDEX map_feature_changeset_commit_sequence ON map_feature_changeset FIELDS commit_sequence;

DEFINE INDEX map_feature_changeset_idempotency_unique ON map_feature_changeset FIELDS tenant, work_context, layer_key, idempotency_key UNIQUE;

DEFINE INDEX map_feature_changeset_key_unique ON map_feature_changeset FIELDS tenant, changeset_key UNIQUE;

DEFINE INDEX map_feature_changeset_layer_sequence ON map_feature_changeset FIELDS tenant, layer_key, commit_sequence;

DEFINE EVENT map_feature_changeset_projection_sequence ON map_feature_changeset WHEN $after.commit_sequence > 0 AND $before.commit_sequence != $after.commit_sequence THEN { LET $head = (SELECT * FROM ONLY map_projection_state:authored_features); IF type::is_object($head) THEN { IF $after.commit_sequence <= $head.last_sequence { THROW 'map_feature_projection_sequence_conflict'; }; UPDATE ONLY map_projection_state:authored_features SET last_sequence = $after.commit_sequence; } ELSE { THROW 'map_feature_projection_sequence_conflict'; } END; };

DEFINE FIELD OVERWRITE bbox_east ON map_feature_head TYPE float PERMISSIONS FULL;

DEFINE FIELD OVERWRITE bbox_north ON map_feature_head TYPE float PERMISSIONS FULL;

DEFINE FIELD OVERWRITE bbox_south ON map_feature_head TYPE float PERMISSIONS FULL;

DEFINE FIELD OVERWRITE bbox_west ON map_feature_head TYPE float PERMISSIONS FULL;

DEFINE FIELD OVERWRITE canonical_json ON map_feature_head TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE changeset_key ON map_feature_head TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE deleted ON map_feature_head TYPE bool PERMISSIONS FULL;

DEFINE FIELD OVERWRITE feature_key ON map_feature_head TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE feature_revision ON map_feature_head TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE geometry_json ON map_feature_head TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE geometry_type ON map_feature_head TYPE 'Point' | 'MultiPoint' | 'LineString' | 'MultiLineString' | 'Polygon' | 'MultiPolygon' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON map_feature_head TYPE array PERMISSIONS FULL;

DEFINE FIELD OVERWRITE layer_key ON map_feature_head TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE layer_revision ON map_feature_head TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE schema_version ON map_feature_head TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE semantic_type ON map_feature_head TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON map_feature_head TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE title ON map_feature_head TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON map_feature_head TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE valid_from ON map_feature_head TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE valid_until ON map_feature_head TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON map_feature_head TYPE record<work_context> PERMISSIONS FULL;

DEFINE INDEX map_feature_head_layer_type ON map_feature_head FIELDS tenant, layer_key, geometry_type;

DEFINE INDEX map_feature_head_layer_updated ON map_feature_head FIELDS tenant, layer_key, deleted, updated_at;

DEFINE INDEX map_feature_head_tenant_key_unique ON map_feature_head FIELDS tenant, layer_key, feature_key UNIQUE;

DEFINE FIELD OVERWRITE archived_at ON map_feature_layer TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority ON map_feature_layer TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.classification ON map_feature_layer TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.context_key ON map_feature_layer TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.data_labels ON map_feature_layer TYPE array<string> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.data_labels.* ON map_feature_layer TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.delegation_id ON map_feature_layer TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants ON map_feature_layer TYPE array<object> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.* ON map_feature_layer TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.permission ON map_feature_layer TYPE 'read' | 'write' | 'admin' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.subject_key ON map_feature_layer TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.subject_kind ON map_feature_layer TYPE 'principal' | 'group' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initiator_key ON map_feature_layer TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.invocation_mode ON map_feature_layer TYPE 'direct' | 'delegated' | 'automated' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.membership ON map_feature_layer TYPE 'viewer' | 'contributor' | 'custodian' | 'owner' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.owner_key ON map_feature_layer TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.owner_kind ON map_feature_layer TYPE 'principal' | 'group' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.policy_revision ON map_feature_layer TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE canonical_json ON map_feature_layer TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE classification ON map_feature_layer TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE content_class ON map_feature_layer TYPE 'reference' | 'named_locations' | 'facilities' | 'boundaries' | 'network_candidate' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON map_feature_layer TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_by_key ON map_feature_layer TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE data_labels ON map_feature_layer TYPE array<string> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE data_labels.* ON map_feature_layer TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE description ON map_feature_layer TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON map_feature_layer TYPE array PERMISSIONS FULL;

DEFINE FIELD OVERWRITE layer_key ON map_feature_layer TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON map_feature_layer TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner_key ON map_feature_layer TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner_kind ON map_feature_layer TYPE 'principal' | 'group' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revision ON map_feature_layer TYPE int ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE schema_revision_key ON map_feature_layer TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE schema_version ON map_feature_layer TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE style_revision_key ON map_feature_layer TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE style_version ON map_feature_layer TYPE none | int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON map_feature_layer TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE title ON map_feature_layer TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON map_feature_layer TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON map_feature_layer TYPE record<work_context> PERMISSIONS FULL;

DEFINE INDEX map_feature_layer_context_updated ON map_feature_layer FIELDS tenant, work_context, archived_at, updated_at;

DEFINE INDEX map_feature_layer_tenant_key_unique ON map_feature_layer FIELDS tenant, layer_key UNIQUE;

DEFINE FIELD OVERWRITE bbox_east ON map_feature_revision TYPE float PERMISSIONS FULL;

DEFINE FIELD OVERWRITE bbox_north ON map_feature_revision TYPE float PERMISSIONS FULL;

DEFINE FIELD OVERWRITE bbox_south ON map_feature_revision TYPE float PERMISSIONS FULL;

DEFINE FIELD OVERWRITE bbox_west ON map_feature_revision TYPE float PERMISSIONS FULL;

DEFINE FIELD OVERWRITE canonical_json ON map_feature_revision TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE changeset_key ON map_feature_revision TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON map_feature_revision TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE deleted ON map_feature_revision TYPE bool PERMISSIONS FULL;

DEFINE FIELD OVERWRITE feature_key ON map_feature_revision TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE feature_revision ON map_feature_revision TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE geometry_json ON map_feature_revision TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE geometry_type ON map_feature_revision TYPE 'Point' | 'MultiPoint' | 'LineString' | 'MultiLineString' | 'Polygon' | 'MultiPolygon' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON map_feature_revision TYPE array PERMISSIONS FULL;

DEFINE FIELD OVERWRITE layer_key ON map_feature_revision TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE layer_revision ON map_feature_revision TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE schema_version ON map_feature_revision TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE semantic_type ON map_feature_revision TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON map_feature_revision TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE title ON map_feature_revision TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE valid_from ON map_feature_revision TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE valid_until ON map_feature_revision TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON map_feature_revision TYPE record<work_context> PERMISSIONS FULL;

DEFINE INDEX map_feature_revision_changeset ON map_feature_revision FIELDS tenant, changeset_key;

DEFINE INDEX map_feature_revision_layer_revision ON map_feature_revision FIELDS tenant, layer_key, layer_revision, feature_key;

DEFINE INDEX map_feature_revision_unique ON map_feature_revision FIELDS tenant, layer_key, feature_key, feature_revision UNIQUE;

DEFINE FIELD OVERWRITE created_at ON map_feature_schema_revision TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_by ON map_feature_schema_revision TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE digest_sha256 ON map_feature_schema_revision TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON map_feature_schema_revision TYPE array PERMISSIONS FULL;

DEFINE FIELD OVERWRITE layer_key ON map_feature_schema_revision TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE schema_json ON map_feature_schema_revision TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE schema_revision_key ON map_feature_schema_revision TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE schema_version ON map_feature_schema_revision TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON map_feature_schema_revision TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON map_feature_schema_revision TYPE record<work_context> PERMISSIONS FULL;

DEFINE INDEX map_feature_schema_key_unique ON map_feature_schema_revision FIELDS tenant, schema_revision_key UNIQUE;

DEFINE INDEX map_feature_schema_version_unique ON map_feature_schema_revision FIELDS tenant, layer_key, schema_version UNIQUE;

DEFINE FIELD OVERWRITE artifact_uri ON map_layer_product TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority ON map_layer_product TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.classification ON map_layer_product TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.context_key ON map_layer_product TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.data_labels ON map_layer_product TYPE array<string> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.data_labels.* ON map_layer_product TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.delegation_id ON map_layer_product TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants ON map_layer_product TYPE array<object> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.* ON map_layer_product TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.permission ON map_layer_product TYPE 'read' | 'write' | 'admin' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.subject_key ON map_layer_product TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.subject_kind ON map_layer_product TYPE 'principal' | 'group' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initiator_key ON map_layer_product TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.invocation_mode ON map_layer_product TYPE 'direct' | 'delegated' | 'automated' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.membership ON map_layer_product TYPE 'viewer' | 'contributor' | 'custodian' | 'owner' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.owner_key ON map_layer_product TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.owner_kind ON map_layer_product TYPE 'principal' | 'group' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.policy_revision ON map_layer_product TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE canonical_json ON map_layer_product TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON map_layer_product TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_by_key ON map_layer_product TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE digest_sha256 ON map_layer_product TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE feature_count ON map_layer_product TYPE int ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE format ON map_layer_product TYPE 'geojson_seq' | 'geoparquet' | 'geo_package' | 'mvt_bundle' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON map_layer_product TYPE array PERMISSIONS FULL;

DEFINE FIELD OVERWRITE layer_key ON map_layer_product TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE layer_revision ON map_layer_product TYPE int ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE mime_type ON map_layer_product TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON map_layer_product TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE product_key ON map_layer_product TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE publication_key ON map_layer_product TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE size_bytes ON map_layer_product TYPE int ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON map_layer_product TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON map_layer_product TYPE record<work_context> PERMISSIONS FULL;

DEFINE INDEX map_layer_product_key_unique ON map_layer_product FIELDS tenant, product_key UNIQUE;

DEFINE INDEX map_layer_product_publication ON map_layer_product FIELDS tenant, work_context, publication_key, created_at;

DEFINE FIELD OVERWRITE artifact_uris ON map_layer_publication TYPE array<string> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE artifact_uris.* ON map_layer_publication TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority ON map_layer_publication TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.classification ON map_layer_publication TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.context_key ON map_layer_publication TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.data_labels ON map_layer_publication TYPE array<string> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.data_labels.* ON map_layer_publication TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.delegation_id ON map_layer_publication TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants ON map_layer_publication TYPE array<object> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.* ON map_layer_publication TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.permission ON map_layer_publication TYPE 'read' | 'write' | 'admin' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.subject_key ON map_layer_publication TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.subject_kind ON map_layer_publication TYPE 'principal' | 'group' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initiator_key ON map_layer_publication TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.invocation_mode ON map_layer_publication TYPE 'direct' | 'delegated' | 'automated' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.membership ON map_layer_publication TYPE 'viewer' | 'contributor' | 'custodian' | 'owner' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.owner_key ON map_layer_publication TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.owner_kind ON map_layer_publication TYPE 'principal' | 'group' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.policy_revision ON map_layer_publication TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE canonical_json ON map_layer_publication TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON map_layer_publication TYPE array PERMISSIONS FULL;

DEFINE FIELD OVERWRITE layer_key ON map_layer_publication TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE layer_revision ON map_layer_publication TYPE int ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON map_layer_publication TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE publication_key ON map_layer_publication TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE published_at ON map_layer_publication TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE published_by_key ON map_layer_publication TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE schema_version ON map_layer_publication TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE style_revision_key ON map_layer_publication TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON map_layer_publication TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON map_layer_publication TYPE record<work_context> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context_key ON map_layer_publication TYPE string PERMISSIONS FULL;

DEFINE INDEX map_layer_publication_key_unique ON map_layer_publication FIELDS tenant, publication_key UNIQUE;

DEFINE INDEX map_layer_publication_layer_revision ON map_layer_publication FIELDS tenant, layer_key, layer_revision, published_at;

DEFINE FIELD OVERWRITE canonical_json ON map_mobility_profile TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON map_mobility_profile TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE family ON map_mobility_profile TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON map_mobility_profile TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE name ON map_mobility_profile TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON map_mobility_profile TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE profile_key ON map_mobility_profile TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE profile_version ON map_mobility_profile TYPE int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON map_mobility_profile TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON map_mobility_profile TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE valid_from ON map_mobility_profile TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE valid_until ON map_mobility_profile TYPE none | datetime PERMISSIONS FULL;

DEFINE INDEX map_profile_tenant_family ON map_mobility_profile FIELDS tenant, family;

DEFINE INDEX map_profile_tenant_version_unique ON map_mobility_profile FIELDS tenant, profile_key, profile_version UNIQUE;

DEFINE FIELD OVERWRITE canonical_json ON map_operational_snapshot TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON map_operational_snapshot TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE departure_time ON map_operational_snapshot TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON map_operational_snapshot TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE snapshot_key ON map_operational_snapshot TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON map_operational_snapshot TYPE record<tenant> PERMISSIONS FULL;

DEFINE INDEX map_snapshot_tenant_departure ON map_operational_snapshot FIELDS tenant, departure_time;

DEFINE INDEX map_snapshot_tenant_key_unique ON map_operational_snapshot FIELDS tenant, snapshot_key UNIQUE;

DEFINE FIELD OVERWRITE id ON map_projection_state TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE last_sequence ON map_projection_state TYPE int ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE affected_mobility_families ON map_restriction TYPE array<string> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE affected_mobility_families.* ON map_restriction TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE cancelled_by ON map_restriction TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE canonical_json ON map_restriction TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON map_restriction TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE effect_kind ON map_restriction TYPE 'prohibit' | 'require' | 'limit' | 'penalize' | 'advise' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON map_restriction TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE kind ON map_restriction TYPE 'closure' | 'access' | 'dimensional_limit' | 'weight_limit' | 'hazardous_cargo' | 'speed_limit' | 'environmental' | 'protected_area' | 'navigational_warning' | 'airspace' | 'weather' | 'other' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON map_restriction TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE record_version ON map_restriction TYPE int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE restriction_key ON map_restriction TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON map_restriction TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON map_restriction TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE valid_from ON map_restriction TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE valid_until ON map_restriction TYPE none | datetime PERMISSIONS FULL;

DEFINE INDEX map_restriction_tenant_key_unique ON map_restriction FIELDS tenant, restriction_key UNIQUE;

DEFINE INDEX map_restriction_tenant_validity ON map_restriction FIELDS tenant, valid_from, valid_until;

DEFINE FIELD OVERWRITE arrival_time ON map_route TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE cache_digest_sha256 ON map_route TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE base_release_ids ON map_route TYPE array<string>;
DEFINE FIELD OVERWRITE restriction_ids ON map_route TYPE array<string>;
DEFINE FIELD OVERWRITE facility_ids ON map_route TYPE array<string>;
DEFINE FIELD OVERWRITE canonical_json ON map_route TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON map_route TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE departure_time ON map_route TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON map_route TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE mobility_profile_key ON map_route TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE mobility_profile_version ON map_route TYPE int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE operational_snapshot_key ON map_route TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON map_route TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE route_key ON map_route TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE status ON map_route TYPE 'planning_advisory' | 'validated' | 'stale' | 'invalidated' | 'unavailable' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON map_route TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON map_route TYPE datetime PERMISSIONS FULL;

DEFINE INDEX map_route_cache ON map_route FIELDS tenant, cache_digest_sha256;

DEFINE INDEX map_route_owner_time ON map_route FIELDS tenant, owner, created_at;

DEFINE INDEX map_route_snapshot ON map_route FIELDS tenant, operational_snapshot_key;

DEFINE INDEX map_route_tenant_key_unique ON map_route FIELDS tenant, route_key UNIQUE;

DEFINE FIELD OVERWRITE created_at ON map_route_dependency TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE dependency_key ON map_route_dependency TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE dependency_kind ON map_route_dependency TYPE 'release' | 'restriction' | 'facility' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON map_route_dependency TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE route_key ON map_route_dependency TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON map_route_dependency TYPE record<tenant> PERMISSIONS FULL;

DEFINE INDEX map_route_dependency_lookup ON map_route_dependency FIELDS tenant, dependency_kind, dependency_key;

DEFINE INDEX map_route_dependency_unique ON map_route_dependency FIELDS tenant, route_key, dependency_kind, dependency_key UNIQUE;

DEFINE FIELD OVERWRITE artifact_uri ON map_route_matrix TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE canonical_json ON map_route_matrix TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON map_route_matrix TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON map_route_matrix TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE matrix_key ON map_route_matrix TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE mobility_profile_key ON map_route_matrix TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE mobility_profile_version ON map_route_matrix TYPE int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE operational_snapshot_key ON map_route_matrix TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON map_route_matrix TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON map_route_matrix TYPE record<tenant> PERMISSIONS FULL;

DEFINE INDEX map_matrix_tenant_key_unique ON map_route_matrix FIELDS tenant, matrix_key UNIQUE;

DEFINE FIELD OVERWRITE adapter_kind ON map_source TYPE 'open_street_map' | 'authority_vector' | 'gtfs_schedule' | 'gtfs_realtime' | 's57_enc' | 's100' | 'aixm' | 'faa_nasr' | 'environmental' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority_class ON map_source TYPE 'regulator' | 'infrastructure_operator' | 'hydrographic_office' | 'air_navigation_service_provider' | 'transport_operator' | 'commercial_provider' | 'community' | 'derived' | 'synthetic_test' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE canonical_json ON map_source TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON map_source TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE dataset_key ON map_source TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE enabled ON map_source TYPE bool PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON map_source TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE map_families ON map_source TYPE array<string> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE map_families.* ON map_source TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE name ON map_source TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON map_source TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE record_version ON map_source TYPE int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE source_key ON map_source TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON map_source TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON map_source TYPE datetime PERMISSIONS FULL;

DEFINE INDEX map_source_tenant_dataset ON map_source FIELDS tenant, dataset_key;

DEFINE INDEX map_source_tenant_enabled ON map_source FIELDS tenant, enabled;

DEFINE INDEX map_source_tenant_key_unique ON map_source FIELDS tenant, source_key UNIQUE;

DEFINE FIELD OVERWRITE created_at ON map_style_revision TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_by ON map_style_revision TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON map_style_revision TYPE array PERMISSIONS FULL;

DEFINE FIELD OVERWRITE layer_key ON map_style_revision TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE style_json ON map_style_revision TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE style_revision_key ON map_style_revision TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE style_version ON map_style_revision TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON map_style_revision TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON map_style_revision TYPE record<work_context> PERMISSIONS FULL;

DEFINE INDEX map_style_key_unique ON map_style_revision FIELDS tenant, style_revision_key UNIQUE;

DEFINE INDEX map_style_version_unique ON map_style_revision FIELDS tenant, layer_key, style_version UNIQUE;

-- Initial state for a fresh owner lane.
CREATE ONLY map_projection_state:authored_features SET last_sequence = 0;
