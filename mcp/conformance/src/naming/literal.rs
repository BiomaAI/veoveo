//! Associate controlled literals with their actual contributing schema constraints.
use super::*;
use std::sync::Arc;

// Narrow adapter for the maintained evaluator's external list output. This is
// internal source association, not a repository-owned wire declaration.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct EvaluationUnit {
    valid: bool,
    evaluation_path: String,
    instance_location: String,
}
#[derive(serde::Deserialize)]
struct EvaluationList {
    details: Vec<EvaluationUnit>,
}
struct Source<'s> {
    schema: &'s Value,
    opaque: bool,
}
impl<'a, 'r> Walker<'a, 'r> {
    fn matches_literal(&mut self, schema: &'r Value, value: &Value) -> Result<bool> {
        self.inspection.tick()?;
        let identity = schema as *const Value as usize;
        if !self.literal_validators.contains_key(&identity) {
            let mut bound = schema.clone();
            bind_key_references(&mut bound)?;
            if let (Some(object), Some(dialect)) =
                (bound.as_object_mut(), self.document.get("$schema"))
            {
                object.entry("$schema").or_insert_with(|| dialect.clone());
            }
            self.literal_validators.insert(
                identity,
                Arc::new(
                    jsonschema::options()
                        .with_registry(self.registry)
                        .build(&bound)?,
                ),
            );
            self.inspection.tick()?;
        }
        let valid = self.literal_validators[&identity].is_valid(value);
        self.inspection.tick()?;
        Ok(valid)
    }
    fn evaluated_by(
        &mut self,
        schema: &'r Value,
        value: &Value,
        keyword: &str,
        location: &str,
    ) -> Result<bool> {
        let key = (
            schema as *const Value as usize,
            value as *const Value as usize,
        );
        if !self.literal_evaluations.contains_key(&key) {
            ensure!(
                self.matches_literal(schema, value)?,
                "invalid literal evaluation"
            );
            let evaluation = self.literal_validators[&key.0].evaluate(value);
            self.inspection.tick()?;
            let output: EvaluationList =
                serde_json::from_value(serde_json::to_value(evaluation.list())?)?;
            let mut locations = BTreeSet::new();
            for unit in output.details {
                self.inspection.tick()?;
                if unit.valid
                    && let Some(keyword) = unit
                        .evaluation_path
                        .strip_prefix('/')
                        .and_then(|path| path.split('/').next())
                    && matches!(keyword, "unevaluatedProperties" | "unevaluatedItems")
                {
                    locations.insert((keyword.to_owned(), unit.instance_location));
                }
            }
            self.literal_evaluations.insert(key, locations);
        }
        self.inspection.tick()?;
        Ok(self.literal_evaluations[&key].contains(&(keyword.to_owned(), location.to_owned())))
    }
    fn pattern_matches(&mut self, pattern: &str, name: &str) -> Result<bool> {
        self.inspection.tick()?;
        let validator =
            jsonschema::validator_for(&serde_json::json!({"type":"string","pattern":pattern}))?;
        let valid = validator.is_valid(&Value::String(name.into()));
        self.inspection.tick()?;
        Ok(valid)
    }
    fn sources(
        &mut self,
        schema: &'r Value,
        value: &Value,
        depth: usize,
        seen: &mut BTreeSet<usize>,
        sources: &mut Vec<Source<'r>>,
    ) -> Result<()> {
        self.inspection.tick()?;
        ensure!(
            depth <= crate::tool_schema::MAX_SCHEMA_DEPTH,
            "literal association depth exceeded"
        );
        if !seen.insert(schema as *const Value as usize) {
            return Ok(());
        }
        if self
            .document
            .get("$schema")
            .and_then(Value::as_str)
            .map(jsonschema::Draft::from_schema_uri)
            .unwrap_or_default()
            == jsonschema::Draft::Draft7
            && let Some(reference) = schema.get("$ref").and_then(Value::as_str)
        {
            let target = self
                .registry
                .resolver(jsonschema::uri::from_str("urn:veoveo:conformance:schema")?)
                .lookup(reference)?;
            return self.sources(target.contents(), value, depth + 1, seen, sources);
        }
        let profile = if schema.is_object() || schema.is_boolean() {
            veoveo_types::naming_profile(&Schema::try_from(schema.clone())?, self.context)?
        } else {
            None
        };
        let opaque = profile.as_ref().is_some_and(|profile| {
            matches!(
                profile.role(),
                NamingRole::External { .. } | NamingRole::Frozen { .. } | NamingRole::Jwt { .. }
            )
        });
        sources.push(Source { schema, opaque });
        if opaque {
            return Ok(());
        }
        if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
            let target = self
                .registry
                .resolver(jsonschema::uri::from_str("urn:veoveo:conformance:schema")?)
                .lookup(reference)?;
            self.sources(target.contents(), value, depth + 1, seen, sources)?;
        }
        for keyword in ["allOf", "anyOf", "oneOf"] {
            if let Some(branches) = schema.get(keyword).and_then(Value::as_array) {
                let mut matched = 0;
                for branch in branches {
                    self.inspection.tick()?;
                    if keyword == "allOf" || self.matches_literal(branch, value)? {
                        matched += 1;
                        self.sources(branch, value, depth + 1, seen, sources)?;
                    }
                }
                ensure!(
                    keyword == "allOf" || matched > 0,
                    "literal has no matching {keyword} schema association"
                );
                ensure!(
                    keyword != "oneOf" || matched == 1,
                    "literal has ambiguous oneOf association"
                );
            }
        }
        if let Some(predicate) = schema.get("if") {
            let keyword = if self.matches_literal(predicate, value)? {
                "then"
            } else {
                "else"
            };
            // The predicate constrains this instance too; its nested name declarations
            // are inspected independently by the ordinary schema walker.
            if let Some(selected) = schema.get(keyword) {
                self.sources(selected, value, depth + 1, seen, sources)?;
            }
        }
        if let Some(instance) = value.as_object() {
            for keyword in ["dependentSchemas", "dependencies"] {
                if let Some(dependencies) = schema.get(keyword).and_then(Value::as_object) {
                    for (name, dependent) in dependencies {
                        self.inspection.tick()?;
                        if instance.contains_key(name) && !dependent.is_array() {
                            self.sources(dependent, value, depth + 1, seen, sources)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
    fn property_sources(&mut self, schema: &'r Value, name: &str) -> Result<Vec<&'r Value>> {
        let mut children = Vec::new();
        if let Some(property) = schema
            .get("properties")
            .and_then(|properties| properties.get(name))
        {
            children.push(property);
        }
        if let Some(patterns) = schema.get("patternProperties").and_then(Value::as_object) {
            for (pattern, child) in patterns {
                if self.pattern_matches(pattern, name)? {
                    children.push(child);
                }
            }
        }
        if children.is_empty()
            && let Some(additional) = schema.get("additionalProperties")
        {
            children.push(additional);
        }
        Ok(children)
    }
    pub(super) fn literal(
        &mut self,
        schema: &'r Value,
        value: &Value,
        path: &str,
        context: Context,
        depth: usize,
        seen: &mut BTreeSet<(Vec<usize>, usize, String)>,
    ) -> Result<()> {
        self.literal_set(&[schema], value, path, context, depth, seen)
    }
    fn literal_set(
        &mut self,
        schemas: &[&'r Value],
        value: &Value,
        path: &str,
        mut context: Context,
        depth: usize,
        seen: &mut BTreeSet<(Vec<usize>, usize, String)>,
    ) -> Result<()> {
        self.inspection.tick()?;
        ensure!(
            depth <= crate::tool_schema::MAX_SCHEMA_DEPTH,
            "controlled literal nesting exceeds depth limit"
        );
        let mut sources = Vec::new();
        let mut source_seen = BTreeSet::new();
        for schema in schemas {
            if schema.is_object() || schema.is_boolean() {
                ensure!(
                    self.matches_literal(schema, value)?,
                    "literal does not satisfy its contributing schema at {path}"
                );
            }
            self.sources(schema, value, depth, &mut source_seen, &mut sources)?;
        }
        // Merge only same-instance scalar/key roles. An opaque branch never
        // grants an exemption to another contributing constraint.
        for source in &sources {
            if source.opaque || !source.schema.is_object() {
                continue;
            }
            if let Some(profile) = veoveo_types::naming_profile(
                &Schema::try_from(source.schema.clone())?,
                self.context,
            )? {
                match profile.role() {
                    NamingRole::Scalar { profile } => {
                        ensure!(
                            context
                                .scalar
                                .as_ref()
                                .is_none_or(|previous| previous == profile),
                            "conflicting literal scalar association"
                        );
                        context.scalar = Some(profile.clone());
                    }
                    NamingRole::Dictionary { key_schema } => {
                        ensure!(
                            context
                                .dictionary
                                .as_ref()
                                .is_none_or(|previous| previous.key == *key_schema.as_value()),
                            "conflicting literal dictionary association"
                        );
                        if context.dictionary.is_none() {
                            let mut bound = key_schema.as_value().clone();
                            bind_key_references(&mut bound)?;
                            context.dictionary = Some(Arc::new(DictionaryContext {
                                key: key_schema.as_value().clone(),
                                identity: serde_json::to_string(key_schema.as_value())?,
                                validator: jsonschema::options()
                                    .with_draft(
                                        self.document
                                            .get("$schema")
                                            .and_then(Value::as_str)
                                            .map(jsonschema::Draft::from_schema_uri)
                                            .unwrap_or_default(),
                                    )
                                    .with_registry(self.registry)
                                    .build(&bound)?,
                            }));
                            self.inspection.tick()?;
                        }
                    }
                    _ => {}
                }
            }
        }
        let identity = (
            sources
                .iter()
                .map(|source| source.schema as *const Value as usize)
                .collect(),
            value as *const Value as usize,
            context.key(),
        );
        if !seen.insert(identity) {
            return Ok(());
        }
        let active: Vec<_> = sources
            .iter()
            .filter(|source| {
                !source.opaque
                    && source.schema != &Value::Bool(true)
                    && !source
                        .schema
                        .as_object()
                        .is_some_and(|object| object.is_empty())
            })
            .collect();
        if active.is_empty() {
            return Ok(());
        }
        match value {
            Value::Object(instance) => {
                for (name, child) in instance {
                    self.inspection.tick()?;
                    let mut children = Vec::new();
                    let mut ordinary_claim = false;
                    let mut opaque_claim = false;
                    for source in &sources {
                        let selected = self.property_sources(source.schema, name)?;
                        if source.opaque {
                            opaque_claim |= !selected.is_empty()
                                || source.schema.get("additionalProperties").is_none();
                            // An opaque declaration applies to the whole subtree,
                            // including fields which upstream leaves open.
                        } else {
                            ordinary_claim |= !selected.is_empty();
                            children.extend(selected);
                        }
                    }
                    if let Some(keys) = &context.dictionary {
                        ensure!(
                            keys.validator.is_valid(&Value::String(name.clone())),
                            "literal dictionary key violates captured schema at {path}/{name}"
                        );
                        self.inspection.tick()?;
                    } else if ordinary_claim || !opaque_claim {
                        ensure!(
                            dto_field_name(name),
                            "invalid controlled literal field at {path}/{name}"
                        );
                    }
                    for source in &active {
                        if let Some(unevaluated) = source.schema.get("unevaluatedProperties") {
                            let location = jsonschema::paths::Location::new().join(name.as_str());
                            if unevaluated == &Value::Bool(true)
                                || self.evaluated_by(
                                    source.schema,
                                    value,
                                    "unevaluatedProperties",
                                    location.as_str(),
                                )?
                            {
                                children.push(unevaluated);
                            }
                        }
                    }
                    if children.is_empty() {
                        if !opaque_claim {
                            self.literal_set(
                                &[&Value::Null],
                                child,
                                path,
                                context.child(),
                                depth + 1,
                                seen,
                            )?;
                        }
                    } else {
                        self.literal_set(&children, child, path, context.child(), depth + 1, seen)?;
                    }
                }
            }
            Value::Array(instance) => {
                for (index, child) in instance.iter().enumerate() {
                    self.inspection.tick()?;
                    let mut children = Vec::new();
                    let mut opaque_claim = false;
                    for source in &sources {
                        if source.opaque {
                            opaque_claim = true;
                            continue;
                        }
                        let prefix = source.schema.get("prefixItems").and_then(Value::as_array);
                        let tuple = source.schema.get("items").and_then(Value::as_array);
                        if let Some(item) = prefix.or(tuple).and_then(|items| items.get(index)) {
                            children.push(item);
                        } else if tuple.is_some() {
                            if let Some(item) = source.schema.get("additionalItems") {
                                children.push(item);
                            }
                        } else if prefix.is_none_or(|items| index >= items.len())
                            && let Some(item) =
                                source.schema.get("items").filter(|item| !item.is_array())
                        {
                            children.push(item);
                        }
                        if let Some(contains) = source.schema.get("contains")
                            && self.matches_literal(contains, child)?
                        {
                            children.push(contains);
                        }
                    }
                    for source in &active {
                        if let Some(item) = source.schema.get("unevaluatedItems") {
                            let location = jsonschema::paths::Location::new().join(index);
                            if item == &Value::Bool(true)
                                || self.evaluated_by(
                                    source.schema,
                                    value,
                                    "unevaluatedItems",
                                    location.as_str(),
                                )?
                            {
                                children.push(item);
                            }
                        }
                    }
                    if !children.is_empty() {
                        self.literal_set(&children, child, path, context.child(), depth + 1, seen)?;
                    } else if !opaque_claim {
                        self.literal_set(
                            &[&Value::Null],
                            child,
                            path,
                            context.child(),
                            depth + 1,
                            seen,
                        )?;
                    }
                }
            }
            Value::String(text) => {
                if let Some(profile) = &context.scalar {
                    self.source_review |=
                        profile.check_spelling(text)? == SpellingCheck::DeclaredSchemaRequired;
                } else if !sources.iter().any(|source| source.opaque) {
                    ensure!(
                        if context.field_values {
                            dto_field_name(text)
                        } else {
                            controlled_name(text)
                        },
                        "invalid controlled literal string at {path}"
                    );
                }
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_evaluation_lookups_exhaust_the_original_work_budget() {
        let schema = Schema::try_from(
            serde_json::json!({"type":"object","unevaluatedProperties":{"type":"integer"}}),
        )
        .unwrap();
        let value = serde_json::json!({"first":1,"second":2});
        let registry = jsonschema::Registry::new()
            .add("urn:veoveo:conformance:schema", schema.as_value())
            .unwrap()
            .prepare()
            .unwrap();
        let mut inspection = NamingInspection::default();
        let mut walker = Walker {
            inspection: &mut inspection,
            context: NamingSchemaContext::new(&schema),
            registry: &registry,
            document: schema.as_value(),
            literal_validators: BTreeMap::new(),
            literal_evaluations: BTreeMap::new(),
            done: BTreeSet::new(),
            visiting: BTreeMap::new(),
            source_review: false,
            reached: BTreeSet::new(),
            definitions: Vec::new(),
        };
        assert!(
            walker
                .evaluated_by(schema.as_value(), &value, "unevaluatedProperties", "/first")
                .unwrap()
        );
        // The actual evaluator/index is already cached. Only subsequent admission
        // lookups can consume this remaining run budget.
        walker.inspection.work = MAX_NAMING_WORK - 1;
        assert!(
            walker
                .evaluated_by(
                    schema.as_value(),
                    &value,
                    "unevaluatedProperties",
                    "/second"
                )
                .unwrap()
        );
        assert!(
            walker
                .evaluated_by(schema.as_value(), &value, "unevaluatedProperties", "/first")
                .is_err()
        );
    }
}
