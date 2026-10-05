DEFINE TABLE own; DEFINE FIELD value ON own TYPE { child: string, nested: { link: record<foreign> } } ASSERT $value.child != NONE;
