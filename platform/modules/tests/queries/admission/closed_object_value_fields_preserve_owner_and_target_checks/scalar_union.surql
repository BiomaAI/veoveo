DEFINE TABLE own; DEFINE FIELD value ON own TYPE string | { child: string } ASSERT $value.child != NONE;
