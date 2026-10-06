DEFINE TABLE own;
DEFINE FIELD identity ON own TYPE object;
DEFINE FIELD identity.*.old ON own TYPE string;
DEFINE INDEX ancestor ON own FIELDS identity.key;
REMOVE FIELD identity.*.old ON own;
DEFINE INDEX preserved ON own FIELDS identity.key;
