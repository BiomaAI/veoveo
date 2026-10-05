DEFINE TABLE own SCHEMAFULL;
DEFINE FIELD name ON own TYPE string ASSERT string::contains($value, (DELETE own));
