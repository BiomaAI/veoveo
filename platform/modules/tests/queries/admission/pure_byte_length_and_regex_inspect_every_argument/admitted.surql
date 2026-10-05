DEFINE TABLE own; DEFINE FIELD method ON own TYPE string ASSERT bytes::len(<bytes>$value) > 0 AND bytes::len(<bytes>$value) <= 256 AND !string::matches($value, '\\p{Cc}');
