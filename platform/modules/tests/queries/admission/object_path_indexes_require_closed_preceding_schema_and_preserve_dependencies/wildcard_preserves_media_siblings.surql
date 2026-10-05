DEFINE TABLE own;
DEFINE FIELD identity ON own TYPE object;
DEFINE FIELD identity.owner_context ON own TYPE object;
DEFINE FIELD identity.owner_context.authority ON own TYPE object;
DEFINE FIELD identity.owner_context.data_labels ON own TYPE array<string>;
DEFINE FIELD identity.owner_context.data_labels.* ON own TYPE string;
DEFINE INDEX tenant ON own FIELDS identity.tenant;
DEFINE INDEX context ON own FIELDS identity.owner_context.authority.work_context;
