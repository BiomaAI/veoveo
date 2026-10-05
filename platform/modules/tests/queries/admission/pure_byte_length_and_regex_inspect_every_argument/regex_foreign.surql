DEFINE TABLE own; DEFINE FIELD method ON own TYPE string ASSERT string::matches($value, <string>(SELECT * FROM foreign));
