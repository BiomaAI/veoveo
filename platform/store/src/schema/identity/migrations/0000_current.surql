-- Current owner schema; schema definitions only.

DEFINE TABLE enterprise TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;


DEFINE TABLE oauth_client TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE principal TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d INCLUDE ORIGINAL PERMISSIONS NONE;

DEFINE TABLE principal_group TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE tenant TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE work_context TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE FIELD OVERWRITE created_at ON enterprise TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE enabled ON enterprise TYPE bool DEFAULT true PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON enterprise TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE name ON enterprise TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE slug ON enterprise TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON enterprise TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE INDEX enterprise_slug_unique ON enterprise FIELDS slug UNIQUE;








DEFINE FIELD OVERWRITE client_id ON oauth_client TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON oauth_client TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE display_name ON oauth_client TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE enabled ON oauth_client TYPE bool DEFAULT true PERMISSIONS FULL;

DEFINE FIELD OVERWRITE grant_types ON oauth_client TYPE array<string> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE grant_types.* ON oauth_client TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON oauth_client TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE kind ON oauth_client TYPE 'public' | 'confidential' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE redirect_uris ON oauth_client TYPE array<string> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE redirect_uris.* ON oauth_client TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE scopes ON oauth_client TYPE array<string> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE scopes.* ON oauth_client TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE secret_hash ON oauth_client TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON oauth_client TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON oauth_client TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE INDEX oauth_client_tenant_client_unique ON oauth_client FIELDS tenant, client_id UNIQUE;

DEFINE INDEX oauth_client_tenant_enabled ON oauth_client FIELDS tenant, enabled;

DEFINE FIELD OVERWRITE claims_hash ON principal TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON principal TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE display_name ON principal TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE email ON principal TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE enabled ON principal TYPE bool DEFAULT true PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON principal TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE issuer ON principal TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE kind ON principal TYPE 'user' | 'service' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE subject ON principal TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON principal TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON principal TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE INDEX principal_external_identity_unique ON principal FIELDS tenant, issuer, subject UNIQUE;

DEFINE INDEX principal_tenant_enabled ON principal FIELDS tenant, enabled;

DEFINE FIELD OVERWRITE created_at ON principal_group TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE display_name ON principal_group TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE external_id ON principal_group TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON principal_group TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON principal_group TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON principal_group TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE INDEX principal_group_external_unique ON principal_group FIELDS tenant, external_id UNIQUE;

DEFINE INDEX principal_group_tenant_name ON principal_group FIELDS tenant, display_name;

DEFINE FIELD OVERWRITE classification_ceiling ON tenant TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON tenant TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE enabled ON tenant TYPE bool DEFAULT true PERMISSIONS FULL;

DEFINE FIELD OVERWRITE enterprise ON tenant TYPE record<enterprise> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON tenant TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE name ON tenant TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE slug ON tenant TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON tenant TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE INDEX tenant_enterprise_enabled ON tenant FIELDS enterprise, enabled;

DEFINE INDEX tenant_enterprise_slug_unique ON tenant FIELDS enterprise, slug UNIQUE;

DEFINE FIELD OVERWRITE context_key ON work_context TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON work_context TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON work_context TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE memberships ON work_context TYPE array<object> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE memberships.* ON work_context TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE memberships.*.groups ON work_context TYPE array<string> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE memberships.*.groups.* ON work_context TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE memberships.*.level ON work_context TYPE 'viewer' | 'contributor' | 'custodian' | 'owner' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE memberships.*.oauth_clients ON work_context TYPE array<string> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE memberships.*.oauth_clients.* ON work_context TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE memberships.*.principals ON work_context TYPE array<string> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE memberships.*.principals.* ON work_context TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE memberships.*.roles ON work_context TYPE array<string> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE memberships.*.roles.* ON work_context TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE output_policy ON work_context TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE output_policy.classification ON work_context TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE output_policy.data_labels ON work_context TYPE array<string> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE output_policy.data_labels.* ON work_context TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE output_policy.initial_grants ON work_context TYPE array<object> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE output_policy.initial_grants.* ON work_context TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE output_policy.initial_grants.*.permission ON work_context TYPE 'read' | 'write' | 'admin' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE output_policy.initial_grants.*.subject_key ON work_context TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE output_policy.initial_grants.*.subject_kind ON work_context TYPE 'principal' | 'group' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE output_policy.owner_key ON work_context TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE output_policy.owner_kind ON work_context TYPE 'principal' | 'group' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE policy_revision ON work_context TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON work_context TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE title ON work_context TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON work_context TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE INDEX work_context_tenant_key_unique ON work_context FIELDS tenant, context_key UNIQUE;

DEFINE INDEX work_context_tenant_title ON work_context FIELDS tenant, title;
