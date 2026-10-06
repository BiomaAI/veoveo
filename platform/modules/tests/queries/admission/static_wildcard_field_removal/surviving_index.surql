DEFINE TABLE own;
DEFINE FIELD identity ON own TYPE object;
DEFINE FIELD identity.child ON own TYPE object;
DEFINE INDEX dependent ON own FIELDS identity.child.key;
REMOVE FIELD identity.*.old ON own;
