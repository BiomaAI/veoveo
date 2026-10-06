DEFINE TABLE own;
DEFINE FIELD identity ON own TYPE object;
DEFINE FIELD identity.child ON own TYPE object;
REMOVE FIELD identity.*.old ON own;
DEFINE INDEX invalidated ON own FIELDS identity.child.key;
