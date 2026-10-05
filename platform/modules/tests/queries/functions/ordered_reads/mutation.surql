DEFINE TABLE own SCHEMAFULL;
DEFINE FUNCTION fn::own::ordered() -> array<object> {
    UPDATE own SET name = 'changed';
    RETURN SELECT name FROM own ORDER BY name ASC LIMIT 20;
} PERMISSIONS FULL;
DEFINE FIELD name ON own TYPE string PERMISSIONS FOR select WHERE count(fn::own::ordered()) > 0;
