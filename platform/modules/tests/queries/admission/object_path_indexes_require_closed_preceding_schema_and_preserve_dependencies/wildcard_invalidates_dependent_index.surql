DEFINE TABLE own;
DEFINE FIELD identity ON own TYPE object;
DEFINE FIELD identity.child ON own TYPE object;
DEFINE INDEX key ON own FIELDS identity.child.key;
DEFINE FIELD identity.* ON own TYPE string;
