-- Current owner schema; schema definitions only.

DEFINE TABLE gateway_authorization_code TYPE NORMAL SCHEMAFULL CHANGEFEED 12w6d PERMISSIONS NONE;

DEFINE TABLE gateway_authorization_request TYPE NORMAL SCHEMAFULL CHANGEFEED 12w6d PERMISSIONS NONE;

DEFINE TABLE gateway_control_active TYPE NORMAL SCHEMAFULL CHANGEFEED 12w6d PERMISSIONS NONE;

DEFINE TABLE gateway_control_object TYPE NORMAL SCHEMAFULL CHANGEFEED 12w6d PERMISSIONS NONE;

DEFINE TABLE gateway_control_revision TYPE NORMAL SCHEMAFULL CHANGEFEED 12w6d PERMISSIONS NONE;

DEFINE TABLE gateway_jwt_revocation TYPE NORMAL SCHEMAFULL CHANGEFEED 12w6d PERMISSIONS NONE;

DEFINE TABLE gateway_refresh_family TYPE NORMAL SCHEMAFULL CHANGEFEED 12w6d PERMISSIONS NONE;

DEFINE TABLE gateway_refresh_token TYPE NORMAL SCHEMAFULL CHANGEFEED 12w6d PERMISSIONS NONE;

DEFINE TABLE gateway_replay_id TYPE NORMAL SCHEMAFULL CHANGEFEED 12w6d PERMISSIONS NONE;

DEFINE TABLE gateway_resource_subscription TYPE NORMAL SCHEMAFULL CHANGEFEED 12w6d PERMISSIONS NONE;

DEFINE TABLE gateway_task_route TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;


DEFINE TABLE mcp_server TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE policy_revision TYPE NORMAL SCHEMAFULL CHANGEFEED 12w6d PERMISSIONS NONE;

DEFINE TABLE profile TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE profile_server TYPE RELATION IN profile OUT mcp_server ENFORCED SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE FIELD OVERWRITE code ON gateway_authorization_code TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE consumed_at ON gateway_authorization_code TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE expires_at ON gateway_authorization_code TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON gateway_authorization_code TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE issued_at ON gateway_authorization_code TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE oauth_client_id ON gateway_authorization_code TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE oidc_client ON gateway_authorization_code TYPE string PERMISSIONS FULL;




DEFINE FIELD OVERWRITE profile ON gateway_authorization_code TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE redirect_uri ON gateway_authorization_code TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON gateway_authorization_code TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE principal ON gateway_authorization_code TYPE object ASSERT object::keys($value) ALLINSIDE ['principal', 'principal_display_name'] AND $value.principal != NONE AND $value.principal_display_name != NONE;
DEFINE FIELD OVERWRITE principal.principal_display_name ON gateway_authorization_code TYPE string;
DEFINE FIELD OVERWRITE principal.principal ON gateway_authorization_code TYPE object ASSERT object::keys($value) ALLINSIDE ['id', 'kind', 'issuer', 'subject', 'tenant', 'groups', 'group_roles', 'roles', 'scopes', 'data_labels', 'assurances', 'authenticated_at'] AND $value.id != NONE AND $value.kind != NONE AND $value.issuer != NONE AND $value.subject != NONE AND $value.groups != NONE AND $value.roles != NONE AND $value.scopes != NONE AND $value.data_labels != NONE AND $value.assurances != NONE;
DEFINE FIELD OVERWRITE principal.principal.id ON gateway_authorization_code TYPE string;
DEFINE FIELD OVERWRITE principal.principal.kind ON gateway_authorization_code TYPE 'user' | 'service';
DEFINE FIELD OVERWRITE principal.principal.issuer ON gateway_authorization_code TYPE string;
DEFINE FIELD OVERWRITE principal.principal.subject ON gateway_authorization_code TYPE string;
DEFINE FIELD OVERWRITE principal.principal.tenant ON gateway_authorization_code TYPE option<string>;
DEFINE FIELD OVERWRITE principal.principal.groups ON gateway_authorization_code TYPE array<string>;
DEFINE FIELD OVERWRITE principal.principal.group_roles ON gateway_authorization_code TYPE option<array<object>>;
DEFINE FIELD OVERWRITE principal.principal.roles ON gateway_authorization_code TYPE array<string>;
DEFINE FIELD OVERWRITE principal.principal.scopes ON gateway_authorization_code TYPE array<string>;
DEFINE FIELD OVERWRITE principal.principal.data_labels ON gateway_authorization_code TYPE array<string>;
DEFINE FIELD OVERWRITE principal.principal.assurances ON gateway_authorization_code TYPE array<'us_person'>;
DEFINE FIELD OVERWRITE principal.principal.authenticated_at ON gateway_authorization_code TYPE option<string>;
DEFINE FIELD OVERWRITE principal.principal.group_roles.* ON gateway_authorization_code TYPE object ASSERT object::keys($value) ALLINSIDE ['group', 'role'] AND $value.group != NONE AND $value.role != NONE;
DEFINE FIELD OVERWRITE principal.principal.group_roles.*.group ON gateway_authorization_code TYPE string;
DEFINE FIELD OVERWRITE principal.principal.group_roles.*.role ON gateway_authorization_code TYPE 'read' | 'write' | 'admin';
DEFINE FIELD OVERWRITE client_state ON gateway_authorization_code TYPE option<string> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE code_challenge ON gateway_authorization_code TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE code_challenge_method ON gateway_authorization_code TYPE 'S256' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE scopes ON gateway_authorization_code TYPE array<string> PERMISSIONS FULL;

DEFINE INDEX gateway_authorization_code_client ON gateway_authorization_code FIELDS profile, oauth_client_id, principal.principal.id, issued_at;

DEFINE INDEX gateway_authorization_code_expiry ON gateway_authorization_code FIELDS expires_at;

DEFINE INDEX gateway_authorization_code_value_unique ON gateway_authorization_code FIELDS code UNIQUE;

DEFINE FIELD OVERWRITE created_at ON gateway_authorization_request TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE expires_at ON gateway_authorization_request TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON gateway_authorization_request TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE idp_state ON gateway_authorization_request TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE oauth_client_id ON gateway_authorization_request TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE oidc_client ON gateway_authorization_request TYPE string PERMISSIONS FULL;


DEFINE FIELD OVERWRITE profile ON gateway_authorization_request TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE redirect_uri ON gateway_authorization_request TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON gateway_authorization_request TYPE string PERMISSIONS FULL;

DEFINE INDEX gateway_authorization_request_client ON gateway_authorization_request FIELDS profile, oauth_client_id, created_at;

DEFINE INDEX gateway_authorization_request_expiry ON gateway_authorization_request FIELDS expires_at;

DEFINE INDEX gateway_authorization_request_state_unique ON gateway_authorization_request FIELDS idp_state UNIQUE;

DEFINE FIELD OVERWRITE id ON gateway_control_active TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revision ON gateway_control_active TYPE record<gateway_control_revision> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revision_id ON gateway_control_active TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON gateway_control_active TYPE datetime PERMISSIONS FULL;

DEFINE INDEX gateway_control_active_revision ON gateway_control_active FIELDS revision UNIQUE;

DEFINE FIELD OVERWRITE document ON gateway_control_object TYPE object FLEXIBLE READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON gateway_control_object TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE object_id ON gateway_control_object TYPE string READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE object_kind ON gateway_control_object TYPE string ASSERT $value = /^[A-Za-z0-9_.:\/-]{1,128}$/ AND ($value != 'profile' OR $this.profile_policy_version != NONE) READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revision ON gateway_control_object TYPE record<gateway_control_revision> READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON gateway_control_object TYPE none | string READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE profile_policy_version ON gateway_control_object TYPE option<string> READONLY ASSERT IF $this.object_kind = 'profile' THEN type::is_string($value) AND string::len($value) > 0 AND $value != /[\s\p{Cc}]/ ELSE $value = NONE END PERMISSIONS FULL;

DEFINE INDEX gateway_control_object_identity_unique ON gateway_control_object FIELDS revision, object_kind, object_id UNIQUE;

DEFINE INDEX gateway_control_object_kind_id ON gateway_control_object FIELDS object_kind, object_id;

DEFINE INDEX gateway_control_object_revision ON gateway_control_object FIELDS revision;

DEFINE INDEX gateway_control_object_tenant_kind ON gateway_control_object FIELDS tenant, object_kind;

DEFINE FIELD OVERWRITE applied_at ON gateway_control_revision TYPE datetime READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE applied_by ON gateway_control_revision TYPE string READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE control_plane ON gateway_control_revision TYPE object FLEXIBLE READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON gateway_control_revision TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revision_id ON gateway_control_revision TYPE string READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE sha256 ON gateway_control_revision TYPE string READONLY ASSERT $value.len() = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE source ON gateway_control_revision TYPE 'admin_api' | 'seed_file' READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON gateway_control_revision TYPE none | string READONLY PERMISSIONS FULL;

DEFINE INDEX gateway_control_revision_applied ON gateway_control_revision FIELDS applied_at, revision_id;

DEFINE INDEX gateway_control_revision_id_unique ON gateway_control_revision FIELDS revision_id UNIQUE;

DEFINE INDEX gateway_control_revision_sha256 ON gateway_control_revision FIELDS sha256;

DEFINE FIELD OVERWRITE expires_at ON gateway_jwt_revocation TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON gateway_jwt_revocation TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE issuer ON gateway_jwt_revocation TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE jwt_id ON gateway_jwt_revocation TYPE string PERMISSIONS FULL;


DEFINE FIELD OVERWRITE profile ON gateway_jwt_revocation TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE reason ON gateway_jwt_revocation TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revoked_at ON gateway_jwt_revocation TYPE datetime PERMISSIONS FULL;

DEFINE INDEX gateway_jwt_revocation_expiry ON gateway_jwt_revocation FIELDS expires_at;

DEFINE INDEX gateway_jwt_revocation_identity_unique ON gateway_jwt_revocation FIELDS profile, issuer, jwt_id UNIQUE;

DEFINE FIELD OVERWRITE authorization_server ON gateway_refresh_family TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE current_generation ON gateway_refresh_family TYPE int ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE expires_at ON gateway_refresh_family TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON gateway_refresh_family TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE issued_at ON gateway_refresh_family TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE oauth_client_id ON gateway_refresh_family TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE principal ON gateway_refresh_family TYPE object ASSERT object::keys($value) ALLINSIDE ['principal', 'principal_display_name'] AND $value.principal != NONE AND $value.principal_display_name != NONE;
DEFINE FIELD OVERWRITE principal.principal_display_name ON gateway_refresh_family TYPE string;
DEFINE FIELD OVERWRITE principal.principal ON gateway_refresh_family TYPE object ASSERT object::keys($value) ALLINSIDE ['id', 'kind', 'issuer', 'subject', 'tenant', 'groups', 'group_roles', 'roles', 'scopes', 'data_labels', 'assurances', 'authenticated_at'] AND $value.id != NONE AND $value.kind != NONE AND $value.issuer != NONE AND $value.subject != NONE AND $value.groups != NONE AND $value.roles != NONE AND $value.scopes != NONE AND $value.data_labels != NONE AND $value.assurances != NONE;
DEFINE FIELD OVERWRITE principal.principal.id ON gateway_refresh_family TYPE string;
DEFINE FIELD OVERWRITE principal.principal.kind ON gateway_refresh_family TYPE 'user' | 'service';
DEFINE FIELD OVERWRITE principal.principal.issuer ON gateway_refresh_family TYPE string;
DEFINE FIELD OVERWRITE principal.principal.subject ON gateway_refresh_family TYPE string;
DEFINE FIELD OVERWRITE principal.principal.tenant ON gateway_refresh_family TYPE option<string>;
DEFINE FIELD OVERWRITE principal.principal.groups ON gateway_refresh_family TYPE array<string>;
DEFINE FIELD OVERWRITE principal.principal.group_roles ON gateway_refresh_family TYPE option<array<object>>;
DEFINE FIELD OVERWRITE principal.principal.roles ON gateway_refresh_family TYPE array<string>;
DEFINE FIELD OVERWRITE principal.principal.scopes ON gateway_refresh_family TYPE array<string>;
DEFINE FIELD OVERWRITE principal.principal.data_labels ON gateway_refresh_family TYPE array<string>;
DEFINE FIELD OVERWRITE principal.principal.assurances ON gateway_refresh_family TYPE array<'us_person'>;
DEFINE FIELD OVERWRITE principal.principal.authenticated_at ON gateway_refresh_family TYPE option<string>;
DEFINE FIELD OVERWRITE principal.principal.group_roles.* ON gateway_refresh_family TYPE object ASSERT object::keys($value) ALLINSIDE ['group', 'role'] AND $value.group != NONE AND $value.role != NONE;
DEFINE FIELD OVERWRITE principal.principal.group_roles.*.group ON gateway_refresh_family TYPE string;
DEFINE FIELD OVERWRITE principal.principal.group_roles.*.role ON gateway_refresh_family TYPE 'read' | 'write' | 'admin';


DEFINE FIELD OVERWRITE principal_id ON gateway_refresh_family TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE profile ON gateway_refresh_family TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revocation_reason ON gateway_refresh_family TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revoked_at ON gateway_refresh_family TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE scopes ON gateway_refresh_family TYPE array<string> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE scopes.* ON gateway_refresh_family TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON gateway_refresh_family TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON gateway_refresh_family TYPE string PERMISSIONS FULL;

DEFINE INDEX gateway_refresh_family_client ON gateway_refresh_family FIELDS authorization_server, profile, oauth_client_id, principal_id, issued_at;

DEFINE INDEX gateway_refresh_family_expiry ON gateway_refresh_family FIELDS expires_at;

DEFINE FIELD OVERWRITE consumed_at ON gateway_refresh_token TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE delivery_envelope ON gateway_refresh_token TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE delivery_expires_at ON gateway_refresh_token TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE expires_at ON gateway_refresh_token TYPE datetime READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE family ON gateway_refresh_token TYPE record<gateway_refresh_family> READONLY REFERENCE ON DELETE CASCADE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE generation ON gateway_refresh_token TYPE int READONLY ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON gateway_refresh_token TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE issued_at ON gateway_refresh_token TYPE datetime READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE replacement ON gateway_refresh_token TYPE none | record<gateway_refresh_token> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE replay_detected_at ON gateway_refresh_token TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE token_hash ON gateway_refresh_token TYPE string READONLY ASSERT $value.len() = 64 PERMISSIONS FULL;

DEFINE INDEX gateway_refresh_token_delivery_expiry ON gateway_refresh_token FIELDS delivery_expires_at;

DEFINE INDEX gateway_refresh_token_expiry ON gateway_refresh_token FIELDS expires_at;

DEFINE INDEX gateway_refresh_token_family_generation_unique ON gateway_refresh_token FIELDS family, generation UNIQUE;

DEFINE INDEX gateway_refresh_token_hash_unique ON gateway_refresh_token FIELDS token_hash UNIQUE;

DEFINE FIELD OVERWRITE authorization_server ON gateway_replay_id TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE client_id ON gateway_replay_id TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE expires_at ON gateway_replay_id TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON gateway_replay_id TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE jwt_id ON gateway_replay_id TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE kind ON gateway_replay_id TYPE 'client_assertion' | 'id_jag' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE seen_at ON gateway_replay_id TYPE datetime PERMISSIONS FULL;

DEFINE INDEX gateway_replay_id_expiry ON gateway_replay_id FIELDS kind, expires_at;

DEFINE INDEX gateway_replay_id_identity_unique ON gateway_replay_id FIELDS kind, authorization_server, client_id, jwt_id UNIQUE;

DEFINE FIELD OVERWRITE created_at ON gateway_resource_subscription TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON gateway_resource_subscription TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON gateway_resource_subscription TYPE string PERMISSIONS FULL;


DEFINE FIELD OVERWRITE profile ON gateway_resource_subscription TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE resource_uri ON gateway_resource_subscription TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON gateway_resource_subscription TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE upstream_server ON gateway_resource_subscription TYPE string PERMISSIONS FULL;

DEFINE INDEX gateway_resource_subscription_identity_unique ON gateway_resource_subscription FIELDS profile, owner, upstream_server, resource_uri UNIQUE;

DEFINE INDEX gateway_resource_subscription_owner ON gateway_resource_subscription FIELDS profile, owner, upstream_server, updated_at;

DEFINE FIELD OVERWRITE authority_digest ON gateway_task_route TYPE string ASSERT string::len($value) = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON gateway_task_route TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE expires_at ON gateway_task_route TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON gateway_task_route TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE ownership ON gateway_task_route TYPE none | object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE ownership.data_labels ON gateway_task_route TYPE array<string> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE ownership.data_labels.* ON gateway_task_route TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE ownership.delegation_id ON gateway_task_route TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE ownership.initiator ON gateway_task_route TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE ownership.invocation_mode ON gateway_task_route TYPE 'direct' | 'delegated' | 'automated' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE ownership.issuer ON gateway_task_route TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE ownership.principal_key ON gateway_task_route TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE ownership.principal_kind ON gateway_task_route TYPE 'user' | 'service' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE ownership.subject ON gateway_task_route TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE ownership.version ON gateway_task_route TYPE int ASSERT $value = 1 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE profile ON gateway_task_route TYPE record<profile> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE server ON gateway_task_route TYPE record<mcp_server> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE source_task ON gateway_task_route TYPE none | record<task> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE source_task_id ON gateway_task_route TYPE string ASSERT string::len($value) > 0 AND string::len($value) <= 512 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON gateway_task_route TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON gateway_task_route TYPE record<work_context> PERMISSIONS FULL;

DEFINE INDEX gateway_task_route_authority ON gateway_task_route FIELDS tenant, owner, work_context, profile, expires_at;

DEFINE INDEX gateway_task_route_source ON gateway_task_route FIELDS server, source_task_id, owner, profile UNIQUE;





















DEFINE FIELD OVERWRITE created_at ON mcp_server TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE display_name ON mcp_server TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE enabled ON mcp_server TYPE bool DEFAULT true PERMISSIONS FULL;

DEFINE FIELD OVERWRITE endpoint ON mcp_server TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON mcp_server TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE manifest ON mcp_server TYPE object FLEXIBLE DEFAULT {  } PERMISSIONS FULL;

DEFINE FIELD OVERWRITE server_key ON mcp_server TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON mcp_server TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE transport ON mcp_server TYPE 'streamable_http' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON mcp_server TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE INDEX mcp_server_tenant_enabled ON mcp_server FIELDS tenant, enabled;

DEFINE INDEX mcp_server_tenant_key_unique ON mcp_server FIELDS tenant, server_key UNIQUE;

DEFINE FIELD OVERWRITE content_hash ON policy_revision TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON policy_revision TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_by ON policy_revision TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE document ON policy_revision TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON policy_revision TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE policy_key ON policy_revision TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE published_at ON policy_revision TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revision ON policy_revision TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE state ON policy_revision TYPE 'draft' | 'active' | 'retired' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON policy_revision TYPE record<tenant> PERMISSIONS FULL;

DEFINE INDEX policy_revision_active ON policy_revision FIELDS tenant, policy_key, state;

DEFINE INDEX policy_revision_content_hash ON policy_revision FIELDS content_hash;

DEFINE INDEX policy_revision_unique ON policy_revision FIELDS tenant, policy_key, revision UNIQUE;

DEFINE FIELD OVERWRITE created_at ON profile TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE description ON profile TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE display_name ON profile TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE enabled ON profile TYPE bool DEFAULT true PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON profile TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE profile_key ON profile TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON profile TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON profile TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE INDEX profile_tenant_enabled ON profile FIELDS tenant, enabled;

DEFINE INDEX profile_tenant_key_unique ON profile FIELDS tenant, profile_key UNIQUE;

DEFINE FIELD OVERWRITE created_at ON profile_server TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE enabled ON profile_server TYPE bool DEFAULT true PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON profile_server TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE in ON profile_server TYPE record<profile> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE namespace ON profile_server TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE out ON profile_server TYPE record<mcp_server> PERMISSIONS FULL;

DEFINE INDEX profile_server_endpoints_unique ON profile_server FIELDS in, out UNIQUE;

DEFINE INDEX profile_server_namespace_unique ON profile_server FIELDS in, namespace UNIQUE;
DEFINE FIELD OVERWRITE client_state ON gateway_authorization_request TYPE option<string> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE code_challenge ON gateway_authorization_request TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE code_challenge_method ON gateway_authorization_request TYPE 'S256' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE requested_scopes ON gateway_authorization_request TYPE array<string> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE idp_code_verifier ON gateway_authorization_request TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE idp_code_challenge ON gateway_authorization_request TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE idp_code_challenge_method ON gateway_authorization_request TYPE 'S256' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE nonce ON gateway_authorization_request TYPE string PERMISSIONS FULL;
