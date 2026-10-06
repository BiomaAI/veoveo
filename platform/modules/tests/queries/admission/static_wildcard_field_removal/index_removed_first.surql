DEFINE TABLE own;
DEFINE FIELD identity ON own TYPE object;
DEFINE FIELD identity.child ON own TYPE object;
DEFINE INDEX dependent ON own FIELDS identity.child.key;
REMOVE INDEX dependent ON own;
REMOVE FIELD identity.*.old ON own;
DEFINE FIELD identity.child ON own TYPE object;
DEFINE INDEX restored ON own FIELDS identity.child.key;
