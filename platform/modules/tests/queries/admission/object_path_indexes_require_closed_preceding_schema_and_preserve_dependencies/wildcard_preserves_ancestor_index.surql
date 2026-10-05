DEFINE TABLE own;
DEFINE FIELD identity ON own TYPE object;
DEFINE INDEX key ON own FIELDS identity.key;
DEFINE FIELD identity.* ON own TYPE string;
