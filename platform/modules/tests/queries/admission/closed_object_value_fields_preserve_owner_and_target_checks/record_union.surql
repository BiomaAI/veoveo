DEFINE TABLE own; DEFINE FIELD value ON own TYPE record<own> | { child: string } ASSERT $value.child != NONE;
