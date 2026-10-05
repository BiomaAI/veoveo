//! Exact SurrealDB 3.3.0 AST adapter. Unsupported syntax always fails admission.
use super::RunnerError;
mod api;
mod effects;
mod functions;
mod index;
mod preparation;
mod values;
use crate::*;
use std::collections::{BTreeMap, BTreeSet};
use surrealdb_sql::ast::TopLevelExpr;
use surrealdb_sql::statements::DefineStatement;
use surrealdb_sql::{Expr, Kind, Literal, Part, Permission, Permissions};

pub(super) fn admit(selection: &ModuleSelection<'_>) -> Result<(), RunnerError> {
    let selected_modules: BTreeSet<_> = selection
        .ordered()
        .iter()
        .map(|module| module.name().clone())
        .collect();
    let preparation = preparation::Preparation::new(selection)?;
    let mut object_fields = BTreeMap::new();
    let mut index_fields = BTreeMap::new();
    let mut roots = functions::DeferredRoots::default();
    for body in &preparation.bodies {
        let mut visitor = Visitor::new(
            selection.registry(),
            &selected_modules,
            &preparation,
            body.module,
            body.migration,
            body.start,
        );
        visitor.object_fields = object_fields.remove(body.module.name()).unwrap_or_default();
        visitor.index_fields = index_fields.remove(body.module.name()).unwrap_or_default();
        for (offset, statement) in body.ast.expressions.iter().enumerate() {
            let TopLevelExpr::Expr(expr) = statement else {
                unreachable!("preparation admits only expression statements")
            };
            visitor.position = body.start + offset;
            visitor.expr(expr, 0).map_err(|error| {
                RunnerError::new(format!(
                    "{} / {}: {}: {}",
                    body.module.name(),
                    body.migration.filename(),
                    construct(expr),
                    error
                ))
            })?;
            let changed = roots.observe(&visitor, expr)?;
            if changed {
                roots.validate(
                    selection.registry(),
                    &selected_modules,
                    &preparation,
                    visitor.position,
                )?;
            }
        }
        object_fields.insert(body.module.name().clone(), visitor.object_fields);
        index_fields.insert(body.module.name().clone(), visitor.index_fields);
    }
    Ok(())
}
fn construct(expr: &Expr) -> &'static str {
    match expr {
        Expr::Define(_) => "DEFINE",
        Expr::Alter(_) => "ALTER",
        Expr::Remove(_) => "REMOVE",
        Expr::Create(_) => "CREATE",
        Expr::Update(_) => "UPDATE",
        Expr::Delete(_) => "DELETE",
        Expr::Select(_) => "SELECT",
        Expr::Return(_) => "RETURN",
        Expr::Let(_) => "LET",
        Expr::FunctionCall(_) => "function call",
        Expr::Idiom(_) => "field or dereference",
        _ => "expression",
    }
}
fn unsupported() -> RunnerError {
    RunnerError::new("SQL construct is outside the admitted migration profile")
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum AccessMode {
    SchemaMutation,
    DataRead,
    DataWrite,
    TypedRecordLink,
    FieldSchemaReference,
    AnalyzerReference,
}
struct Visitor<'a> {
    registry: &'a ModuleRegistry,
    preparation: &'a preparation::Preparation<'a>,
    position: usize,
    deferred: bool,
    function_body: bool,
    this_table: Option<TableName>,
    field_value: Option<Kind>,
    selected_modules: &'a BTreeSet<ModuleName>,
    module: &'a ModuleSetup,
    nodes: usize,
    migration: &'a Migration,
    api: Option<&'a KernelSqlApi>,
    parameters: BTreeMap<String, Option<SqlType>>,
    objects: BTreeSet<String>,
    argument_readonly: bool,
    row_scope: bool,
    object_fields: BTreeSet<(String, Vec<String>)>,
    index_fields: BTreeMap<(String, String), Vec<Vec<String>>>,
}
impl<'a> Visitor<'a> {
    fn new(
        registry: &'a ModuleRegistry,
        selected_modules: &'a BTreeSet<ModuleName>,
        preparation: &'a preparation::Preparation<'a>,
        module: &'a ModuleSetup,
        migration: &'a Migration,
        position: usize,
    ) -> Self {
        Self {
            registry,
            selected_modules,
            preparation,
            position,
            deferred: false,
            function_body: false,
            this_table: None,
            field_value: None,
            module,
            migration,
            nodes: 0,
            api: None,
            parameters: BTreeMap::new(),
            objects: BTreeSet::new(),
            argument_readonly: false,
            row_scope: false,
            object_fields: BTreeSet::new(),
            index_fields: BTreeMap::new(),
        }
    }

    fn object(&self, kind: ObjectKind, name: &str, mode: AccessMode) -> Result<(), RunnerError> {
        if let Some(api) = self.api {
            if mode == AccessMode::DataRead
                && (kind != ObjectKind::Table
                    || !api.reads().tables().iter().any(|t| t.as_str() == name))
            {
                return Err(RunnerError::new(
                    "SQL API read is outside its owned read profile",
                ));
            }
            if mode == AccessMode::SchemaMutation {
                return Err(RunnerError::new("SQL API cannot mutate schema"));
            }
            if mode == AccessMode::DataWrite
                && !matches!(api.effects(), SqlEffectProfile::OwnedUpdate(profile)
                    if kind == ObjectKind::Table && profile.table().as_str() == name)
            {
                return Err(RunnerError::new(
                    "SQL API write is outside its owned update profile",
                ));
            }
        }
        let valid = match kind {
            ObjectKind::Table => TableName::new(name).is_ok(),
            ObjectKind::Function => FunctionName::new(name).is_ok(),
            ObjectKind::Analyzer => AnalyzerName::new(name).is_ok(),
        };
        if !valid {
            return Err(RunnerError::new("invalid static object name"));
        }
        if kind == ObjectKind::Table
            && [LANE_TABLE, MIGRATION_TABLE, PREPARATION_TABLE].contains(&name)
        {
            return Err(RunnerError::new(
                "user migrations cannot access runner history tables",
            ));
        }
        let owner = self
            .registry
            .owner(kind, name)
            .ok_or_else(|| RunnerError::new(format!("unclaimed {:?} object {name}", kind)))?;
        if owner.name() == self.module.name() {
            return Ok(());
        }
        // Plain field record types describe schema links, not execution prerequisites.
        // All selected kernels must be installed before runtime values are admitted.
        if mode == AccessMode::FieldSchemaReference
            && self.module.layer() == ModuleLayer::Kernel
            && owner.layer() == ModuleLayer::Kernel
            && self.selected_modules.contains(owner.name())
        {
            return Ok(());
        }
        if self.registry.depends_on(self.module.name(), owner.name()) {
            match mode {
                AccessMode::TypedRecordLink
                | AccessMode::FieldSchemaReference
                | AccessMode::AnalyzerReference => return Ok(()),
                AccessMode::DataRead
                    if self.module.layer() == ModuleLayer::Kernel
                        || owner.layer() == ModuleLayer::Optional =>
                {
                    return Ok(());
                }
                _ => {}
            }
        }
        Err(RunnerError::new(format!(
            "{:?} object {name} violates ownership or layer rules",
            kind
        )))
    }
    fn name<'expr>(&self, expr: &'expr Expr) -> Result<&'expr str, RunnerError> {
        match expr {
            Expr::Table(n) => Ok(n.as_str()),
            Expr::Idiom(i) if i.0.len() == 1 => match &i.0[0] {
                Part::Field(n) => Ok(n.as_str()),
                _ => Err(unsupported()),
            },
            Expr::Literal(Literal::String(n)) => Ok(n.as_str()),
            _ => Err(unsupported()),
        }
    }
    fn target(&mut self, expr: &Expr, write: bool, depth: usize) -> Result<(), RunnerError> {
        match expr {
            Expr::Param(parameter) if self.api.is_some() || self.function_body => {
                let Some(Some(SqlType::Record(table))) = self.parameters.get(parameter.as_str())
                else {
                    return Err(unsupported());
                };
                self.object(
                    ObjectKind::Table,
                    table.as_str(),
                    if write {
                        AccessMode::DataWrite
                    } else {
                        AccessMode::DataRead
                    },
                )
            }
            Expr::Literal(Literal::RecordId(record)) => {
                // Only scalar keys are admitted; expression-bearing keys are rejected.
                self.object(
                    ObjectKind::Table,
                    record.table.as_str(),
                    if write {
                        AccessMode::DataWrite
                    } else {
                        AccessMode::DataRead
                    },
                )?;
                self.record_key(&record.key, depth)
            }
            _ => self.object(
                ObjectKind::Table,
                self.name(expr)?,
                if write {
                    AccessMode::DataWrite
                } else {
                    AccessMode::DataRead
                },
            ),
        }
    }
    fn idiom(&mut self, idiom: &surrealdb_sql::Idiom, depth: usize) -> Result<(), RunnerError> {
        match idiom.0.as_slice() {
            [Part::Field(_) | Part::All] if self.api.is_none() || self.row_scope => Ok(()),
            [Part::Start(Expr::Param(p)), Part::Field(_)] if self.objects.contains(p.as_str()) => {
                self.expr(&Expr::Param(p.clone()), depth + 1)
            }
            _ => self.value_idiom(idiom, depth),
        }
    }

    fn permissions(&mut self, p: &Permissions, depth: usize) -> Result<(), RunnerError> {
        for permission in [&p.select, &p.create, &p.update, &p.delete] {
            self.permission(permission, depth)?;
        }
        Ok(())
    }
    fn permission(&mut self, p: &Permission, depth: usize) -> Result<(), RunnerError> {
        match p {
            Permission::None | Permission::Full => Ok(()),
            Permission::Specific(expr) => {
                let deferred = self.deferred;
                self.deferred = true;
                let result = self.readonly_expr(expr, depth + 1);
                self.deferred = deferred;
                result
            }
        }
    }
    fn readonly_expr(&mut self, expr: &Expr, depth: usize) -> Result<(), RunnerError> {
        let readonly = self.argument_readonly;
        self.argument_readonly = true;
        let result = self.expr(expr, depth);
        self.argument_readonly = readonly;
        result
    }
    fn expr(&mut self, expr: &Expr, depth: usize) -> Result<(), RunnerError> {
        self.expr_inner(expr, depth)
            .map_err(|error| RunnerError::new(format!("{}: {error}", construct(expr))))
    }
    fn expr_inner(&mut self, expr: &Expr, depth: usize) -> Result<(), RunnerError> {
        self.nodes += 1;
        if depth > 64 || self.nodes > 100_000 {
            return Err(RunnerError::new("SQL admission complexity budget exceeded"));
        }
        if self.function_body && matches!(expr, Expr::Define(_) | Expr::Remove(_) | Expr::Alter(_))
        {
            return Err(RunnerError::new("stored function cannot mutate schema"));
        }
        self.check_effect(expr)?;
        match expr {
            Expr::Literal(literal) => self.literal(literal, depth),
            Expr::Param(p)
                if (self.api.is_some() || self.function_body)
                    && !self.parameters.contains_key(p.as_str()) =>
            {
                Err(RunnerError::new("undeclared SQL API parameter"))
            }
            Expr::Param(_) | Expr::Constant(_) => Ok(()),
            Expr::Idiom(i) => self.idiom(i, depth),
            Expr::Table(name) => {
                self.object(ObjectKind::Table, name.as_str(), AccessMode::DataRead)
            }
            Expr::Prefix { expr, op } => {
                if let surrealdb_sql::PrefixOperator::Cast(kind) = op {
                    self.kind(kind, depth + 1)?;
                }
                self.expr(expr, depth + 1)
            }
            Expr::Postfix { .. } => Err(unsupported()),
            Expr::Binary { left, right, .. } => {
                self.expr(left, depth + 1)?;
                self.expr(right, depth + 1)
            }
            Expr::Block(block) => {
                let parameters = self.parameters.clone();
                let objects = self.objects.clone();
                for expr in &block.0 {
                    self.expr(expr, depth + 1)?;
                }
                self.parameters = parameters;
                self.objects = objects;
                Ok(())
            }
            Expr::IfElse(statement) => self.guarded_if(statement, depth + 1),
            Expr::Closure(closure) => self.closure(closure, depth + 1),
            Expr::Foreach(statement) => {
                self.expr(&statement.range, depth + 1)?;
                let parameters = self.parameters.clone();
                let objects = self.objects.clone();
                if self.parameters.contains_key(statement.param.as_str()) {
                    return Err(RunnerError::new("FOR cannot shadow an admitted local"));
                }
                let item = if let Expr::Param(range) = &statement.range {
                    match self.parameters.get(range.as_str()) {
                        Some(Some(SqlType::Array(inner))) => Some(inner.as_ref().clone()),
                        _ => None,
                    }
                } else {
                    None
                };
                if item == Some(SqlType::Object) {
                    self.objects.insert(statement.param.as_str().into());
                }
                self.parameters
                    .insert(statement.param.as_str().into(), item);
                for expr in &statement.block.0 {
                    self.expr(expr, depth + 1)?;
                }
                self.parameters = parameters;
                self.objects = objects;
                Ok(())
            }
            Expr::Throw(expr) => self.expr(expr, depth + 1),
            Expr::Return(output) => {
                if output.fetch.is_some() {
                    return Err(unsupported());
                }
                self.expr(&output.what, depth + 1)
            }
            Expr::Let(set) => {
                if let Some(kind) = &set.kind {
                    self.kind(kind, depth + 1)?;
                }
                self.expr(&set.what, depth + 1)?;
                let name = set.name.as_str().to_owned();
                if (self.api.is_some() || self.function_body) && self.parameters.contains_key(&name)
                {
                    return Err(RunnerError::new(
                        "SQL API LET cannot redefine a parameter or local",
                    ));
                }
                let kind = set.kind.as_ref().and_then(api::sql_type);
                if kind == Some(SqlType::Object) {
                    self.objects.insert(name.clone());
                } else {
                    self.objects.remove(&name);
                }
                self.parameters.insert(name, kind);
                Ok(())
            }
            Expr::FunctionCall(call) => {
                // Inspect every argument even when the receiver is rejected; custom arguments cannot mutate.
                let readonly = self.argument_readonly;
                self.argument_readonly = true;
                for arg in &call.arguments {
                    self.expr(arg, depth + 1)?;
                }
                self.argument_readonly = readonly;
                if matches!(&call.receiver, surrealdb_sql::Function::Normal(name) if name == "type::record")
                {
                    let [Expr::Literal(Literal::String(table)), _] = call.arguments.as_slice()
                    else {
                        return Err(unsupported());
                    };
                    return self.object(
                        ObjectKind::Table,
                        table.as_str(),
                        AccessMode::TypedRecordLink,
                    );
                }
                if matches!(&call.receiver, surrealdb_sql::Function::Normal(name) if name == "record::exists")
                {
                    let [target] = call.arguments.as_slice() else {
                        return Err(unsupported());
                    };
                    return self.target(target, false, depth + 1);
                }
                match &call.receiver {
                    surrealdb_sql::Function::Custom(name) => {
                        return self.api_call(name, &call.arguments, depth + 1);
                    }
                    surrealdb_sql::Function::Normal(name)
                        if [
                            "array::len",
                            "record::tb",
                            "record::id",
                            "object::keys",
                            "time::floor",
                            "count",
                            "crypto::sha256",
                            "array::first",
                            "type::is_object",
                            "type::is_array",
                            "string::len",
                            "string::lowercase",
                            "string::contains",
                            "string::uppercase",
                            "math::abs",
                            "math::floor",
                            "type::is_string",
                            "type::string",
                            "time::now",
                            "rand::uuid::v7",
                            "rand::uuid",
                        ]
                        .contains(&name.as_str()) => {}
                    surrealdb_sql::Function::Normal(name) => {
                        return Err(RunnerError::new(format!(
                            "builtin {name} is outside the admitted migration profile"
                        )));
                    }
                    _ => return Err(unsupported()),
                }
                if self.api.is_some()
                    && matches!(&call.receiver, surrealdb_sql::Function::Normal(name) if ["time::now", "rand::uuid::v7", "rand::uuid"].contains(&name.as_str()))
                {
                    return Err(unsupported());
                }
                Ok(())
            }
            Expr::Define(statement) => self.define(statement, depth + 1),
            Expr::Remove(statement) => self.remove(statement, depth + 1),
            Expr::Alter(statement) => self.alter(statement, depth + 1),
            Expr::Create(statement) => {
                for target in &statement.what {
                    self.target(target, true, depth + 1)?;
                }
                if let Some(data) = &statement.data {
                    self.data(data, depth + 1)?;
                }
                self.output(statement.output.as_ref(), depth + 1)?;
                self.expr(&statement.timeout, depth + 1)
            }
            Expr::Update(statement) => {
                if statement.with.is_some() || statement.explain.is_some() {
                    return Err(unsupported());
                }
                for target in &statement.what {
                    self.target(target, true, depth + 1)?;
                }
                let row_scope = self.row_scope;
                self.row_scope = true;
                if let Some(data) = &statement.data {
                    self.data(data, depth + 1)?;
                }
                if let Some(cond) = &statement.cond {
                    self.expr(&cond.0, depth + 1)?;
                }
                self.output(statement.output.as_ref(), depth + 1)?;
                self.expr(&statement.timeout, depth + 1)?;
                self.row_scope = row_scope;
                Ok(())
            }
            Expr::Delete(statement) => {
                if statement.with.is_some() || statement.explain.is_some() {
                    return Err(unsupported());
                }
                for target in &statement.what {
                    self.target(target, true, depth + 1)?;
                }
                if let Some(cond) = &statement.cond {
                    self.expr(&cond.0, depth + 1)?;
                }
                self.output(statement.output.as_ref(), depth + 1)?;
                self.expr(&statement.timeout, depth + 1)
            }
            Expr::Select(statement) => self.select(statement, depth + 1),
            _ => Err(unsupported()),
        }
    }
    fn define(&mut self, statement: &DefineStatement, depth: usize) -> Result<(), RunnerError> {
        if depth != 1 {
            return Err(RunnerError::new(
                "schema declarations must be migration top-level statements",
            ));
        }
        let conditional = match statement {
            DefineStatement::Table(s) => &s.kind,
            DefineStatement::Field(s) => &s.kind,
            DefineStatement::Index(s) => &s.kind,
            DefineStatement::Function(s) => &s.kind,
            DefineStatement::Event(s) => &s.kind,
            _ => &surrealdb_sql::statements::define::DefineKind::Default,
        };
        if matches!(
            conditional,
            surrealdb_sql::statements::define::DefineKind::IfNotExists
        ) {
            return Err(RunnerError::new(
                "conditional deferred definitions cannot prove stored body identity",
            ));
        }
        match statement {
            DefineStatement::Table(s) => {
                let table = self.name(&s.name)?.to_owned();
                self.clear_table_proof(&table, false)?;
                self.object(
                    ObjectKind::Table,
                    self.name(&s.name)?,
                    AccessMode::SchemaMutation,
                )?;
                if s.graphql_alias.is_some()
                    || s.graphql_deprecated.is_some()
                    || s.inline_edges_cap.is_some()
                    || s.inline_refs_cap.is_some()
                {
                    return Err(unsupported());
                }
                if let surrealdb_sql::TableType::Relation(relation) = &s.table_type {
                    if relation.from.is_empty() || relation.to.is_empty() || relation.lightweight {
                        return Err(unsupported());
                    }
                    for target in relation.from.iter().chain(&relation.to) {
                        self.object(
                            ObjectKind::Table,
                            target.as_str(),
                            AccessMode::TypedRecordLink,
                        )?;
                    }
                }
                if let Some(view) = &s.view {
                    self.view(view, depth + 1)?;
                }
                self.permissions(&s.permissions, depth)?;
                self.readonly_expr(&s.comment, depth)
            }
            DefineStatement::Field(s) => {
                self.field_proof(s, depth)?;
                self.object(
                    ObjectKind::Table,
                    self.name(&s.what)?,
                    AccessMode::SchemaMutation,
                )?;
                // Static field idioms only: a field name must never execute a subquery.
                match &s.name {
                    Expr::Idiom(i)
                        if i.0.iter().all(|p| matches!(p, Part::Field(_) | Part::All)) => {}
                    _ => return Err(unsupported()),
                }
                if s.graphql_alias.is_some() || s.graphql_deprecated.is_some() {
                    return Err(unsupported());
                }
                if let Some(kind) = &s.field_kind {
                    self.kind_with_mode(
                        kind,
                        depth,
                        if s.reference.is_none() {
                            AccessMode::FieldSchemaReference
                        } else {
                            AccessMode::TypedRecordLink
                        },
                    )?;
                }
                let this_table = self
                    .this_table
                    .replace(TableName::new(self.name(&s.what)?).map_err(|_| unsupported())?);
                let field_value = std::mem::replace(&mut self.field_value, s.field_kind.clone());
                let deferred = self.deferred;
                self.deferred = true;
                if let Some(reference) = &s.reference {
                    match &reference.on_delete {
                        surrealdb_sql::reference::ReferenceDeleteStrategy::Reject
                        | surrealdb_sql::reference::ReferenceDeleteStrategy::Ignore
                        | surrealdb_sql::reference::ReferenceDeleteStrategy::Cascade
                        | surrealdb_sql::reference::ReferenceDeleteStrategy::Unset => {}
                        surrealdb_sql::reference::ReferenceDeleteStrategy::Custom(expr) => {
                            self.expr(expr, depth + 1)?
                        }
                    }
                }
                for expr in [&s.value, &s.assert, &s.computed].into_iter().flatten() {
                    self.readonly_expr(expr, depth + 1)?;
                }
                match &s.default {
                    surrealdb_sql::statements::define::DefineDefault::None => {}
                    surrealdb_sql::statements::define::DefineDefault::Always(expr)
                    | surrealdb_sql::statements::define::DefineDefault::Set(expr) => {
                        self.readonly_expr(expr, depth + 1)?
                    }
                }
                self.permissions(&s.permissions, depth)?;
                self.deferred = deferred;
                self.this_table = this_table;
                self.field_value = field_value;
                self.readonly_expr(&s.comment, depth)
            }
            DefineStatement::Index(s) => {
                self.object(
                    ObjectKind::Table,
                    self.name(&s.what)?,
                    AccessMode::SchemaMutation,
                )?;
                self.name(&s.name)?;
                if s.concurrently {
                    return Err(unsupported());
                }
                self.index_proof(s, depth + 1)?;
                match &s.index {
                    surrealdb_sql::Index::Idx
                    | surrealdb_sql::Index::Uniq
                    | surrealdb_sql::Index::Hnsw(_) => {}
                    surrealdb_sql::Index::FullText(params) => self.object(
                        ObjectKind::Analyzer,
                        params.az.as_str(),
                        AccessMode::AnalyzerReference,
                    )?,
                    surrealdb_sql::Index::Count(condition) => {
                        if let Some(c) = condition {
                            self.readonly_expr(&c.0, depth + 1)?;
                        }
                    }
                    _ => return Err(unsupported()),
                }
                self.readonly_expr(&s.comment, depth)
            }
            DefineStatement::Analyzer(s) => {
                self.object(
                    ObjectKind::Analyzer,
                    self.name(&s.name)?,
                    AccessMode::SchemaMutation,
                )?;
                // Callbacks can invoke arbitrary stored functions; no callback is qualified yet.
                if s.function.is_some() {
                    return Err(RunnerError::new(
                        "analyzer callback requires body qualification",
                    ));
                }
                if let Some(tokenizers) = &s.tokenizers {
                    for tokenizer in tokenizers {
                        match tokenizer {
                            surrealdb_sql::tokenizer::Tokenizer::Blank
                            | surrealdb_sql::tokenizer::Tokenizer::Camel
                            | surrealdb_sql::tokenizer::Tokenizer::Class
                            | surrealdb_sql::tokenizer::Tokenizer::Punct => {}
                            surrealdb_sql::tokenizer::Tokenizer::Segment(_) => {
                                return Err(RunnerError::new(
                                    "dictionary tokenizer profile is not qualified",
                                ));
                            }
                        }
                    }
                }
                if let Some(filters) = &s.filters {
                    for filter in filters {
                        match filter {
                            surrealdb_sql::filter::Filter::Ascii
                            | surrealdb_sql::filter::Filter::EdgeNgram(_, _)
                            | surrealdb_sql::filter::Filter::Lowercase
                            | surrealdb_sql::filter::Filter::Ngram(_, _)
                            | surrealdb_sql::filter::Filter::Snowball(_)
                            | surrealdb_sql::filter::Filter::Uppercase => {}
                            surrealdb_sql::filter::Filter::Mapper(_) => {
                                return Err(RunnerError::new(
                                    "analyzer MAPPER external file effect rejected",
                                ));
                            }
                        }
                    }
                }
                self.readonly_expr(&s.comment, depth)
            }
            DefineStatement::Function(s) => self.function_definition(s, depth + 1),
            DefineStatement::Event(s) => {
                self.object(
                    ObjectKind::Table,
                    self.name(&s.target_table)?,
                    AccessMode::SchemaMutation,
                )?;
                self.name(&s.name)?;
                if !matches!(s.event_kind, surrealdb_sql::EventKind::Sync) {
                    return Err(unsupported());
                }
                let objects = self.objects.clone();
                let parameters = self.parameters.clone();
                let deferred = self.deferred;
                self.deferred = true;
                for name in ["before", "after"] {
                    self.objects.insert(name.into());
                    self.parameters.insert(name.into(), Some(SqlType::Object));
                }
                self.readonly_expr(&s.when, depth + 1)?;
                for expr in &s.then {
                    self.expr(expr, depth + 1)?;
                }
                self.objects = objects;
                self.parameters = parameters;
                self.deferred = deferred;
                self.readonly_expr(&s.comment, depth)
            }
            _ => Err(unsupported()),
        }
    }
    fn remove(
        &mut self,
        s: &surrealdb_sql::statements::RemoveStatement,
        depth: usize,
    ) -> Result<(), RunnerError> {
        use surrealdb_sql::statements::RemoveStatement;
        if depth != 1 {
            return Err(RunnerError::new(
                "schema removals must be migration top-level statements",
            ));
        }
        match s {
            RemoveStatement::Table(s) => {
                let table = self.name(&s.name)?.to_owned();
                self.clear_table_proof(&table, true)?;
                if s.expunge {
                    return Err(unsupported());
                }
                self.object(
                    ObjectKind::Table,
                    self.name(&s.name)?,
                    AccessMode::SchemaMutation,
                )
            }
            RemoveStatement::Field(s) => {
                let table = self.name(&s.what)?.to_owned();
                let path = index::path(&s.name).ok_or_else(unsupported)?;
                self.invalidate_field(&table, &path, false)?;
                self.static_field(&s.name)?;
                self.object(
                    ObjectKind::Table,
                    self.name(&s.what)?,
                    AccessMode::SchemaMutation,
                )
            }
            RemoveStatement::Index(s) => {
                self.index_fields
                    .remove(&(self.name(&s.what)?.into(), self.name(&s.name)?.into()));
                self.name(&s.name)?;
                self.object(
                    ObjectKind::Table,
                    self.name(&s.what)?,
                    AccessMode::SchemaMutation,
                )
            }
            RemoveStatement::Event(s) => {
                self.name(&s.name)?;
                self.object(
                    ObjectKind::Table,
                    self.name(&s.what)?,
                    AccessMode::SchemaMutation,
                )
            }
            RemoveStatement::Function(s) => self.object(
                ObjectKind::Function,
                &format!("fn::{}", s.name),
                AccessMode::SchemaMutation,
            ),
            RemoveStatement::Analyzer(s) => self.object(
                ObjectKind::Analyzer,
                self.name(&s.name)?,
                AccessMode::SchemaMutation,
            ),
            _ => Err(unsupported()),
        }
    }
    fn static_field(&self, expr: &Expr) -> Result<(), RunnerError> {
        match expr {
            Expr::Idiom(i) if i.0.iter().all(|p| matches!(p, Part::Field(_) | Part::All)) => Ok(()),
            _ => Err(unsupported()),
        }
    }
    fn alter(
        &mut self,
        s: &surrealdb_sql::statements::AlterStatement,
        depth: usize,
    ) -> Result<(), RunnerError> {
        use surrealdb_sql::statements::AlterStatement;
        if depth != 1 {
            return Err(RunnerError::new(
                "schema-shape alterations must be migration top-level statements",
            ));
        }
        match s {
            AlterStatement::Table(s) => {
                self.object(
                    ObjectKind::Table,
                    self.name(&s.name)?,
                    AccessMode::SchemaMutation,
                )?;
                if s.compact
                    || s.kind.as_ref().is_some_and(|k| {
                        !matches!(
                            k,
                            surrealdb_sql::TableType::Normal | surrealdb_sql::TableType::Any
                        )
                    })
                {
                    return Err(unsupported());
                }
                if let Some(p) = &s.permissions {
                    self.permissions(p, depth + 1)?;
                }
                Ok(())
            }
            _ => Err(unsupported()),
        }
    }
    fn kind(&mut self, kind: &Kind, depth: usize) -> Result<(), RunnerError> {
        self.kind_with_mode(kind, depth, AccessMode::TypedRecordLink)
    }
    fn kind_with_mode(
        &mut self,
        kind: &Kind,
        depth: usize,
        mode: AccessMode,
    ) -> Result<(), RunnerError> {
        if depth > 64 {
            return Err(unsupported());
        }
        match kind {
            Kind::Record(tables) | Kind::Table(tables) => {
                if tables.is_empty() {
                    return if matches!(kind, Kind::Record(_))
                        && mode == AccessMode::FieldSchemaReference
                    {
                        Ok(())
                    } else {
                        Err(unsupported())
                    };
                }
                for table in tables {
                    self.object(
                        ObjectKind::Table,
                        table.as_str(),
                        if matches!(kind, Kind::Record(_)) {
                            mode
                        } else {
                            AccessMode::TypedRecordLink
                        },
                    )?;
                }
                Ok(())
            }
            Kind::Either(kinds) => {
                for kind in kinds {
                    self.kind_with_mode(kind, depth + 1, mode)?;
                }
                Ok(())
            }
            Kind::Array(kind, _) | Kind::Set(kind, _) => self.kind_with_mode(kind, depth + 1, mode),
            Kind::Literal(literal) => {
                use surrealdb_sql::kind::KindLiteral;
                match literal {
                    KindLiteral::Array(kinds) => {
                        for kind in kinds {
                            self.kind_with_mode(kind, depth + 1, mode)?;
                        }
                    }
                    KindLiteral::Object(fields) => {
                        for kind in fields.values() {
                            self.kind_with_mode(kind, depth + 1, mode)?;
                        }
                    }
                    KindLiteral::Float(value) if !value.is_finite() => return Err(unsupported()),
                    KindLiteral::String(_)
                    | KindLiteral::Integer(_)
                    | KindLiteral::Float(_)
                    | KindLiteral::Decimal(_)
                    | KindLiteral::Duration(_)
                    | KindLiteral::Bool(_) => {}
                }
                Ok(())
            }
            Kind::Any
            | Kind::None
            | Kind::Null
            | Kind::Bool
            | Kind::Bytes
            | Kind::Datetime
            | Kind::Decimal
            | Kind::Duration
            | Kind::Float
            | Kind::Int
            | Kind::Number
            | Kind::Object
            | Kind::String
            | Kind::Uuid
            | Kind::Regex
            | Kind::Geometry(_)
            | Kind::Range => Ok(()),
            _ => Err(unsupported()),
        }
    }
    fn record_key(
        &mut self,
        key: &surrealdb_sql::RecordIdKeyLit,
        depth: usize,
    ) -> Result<(), RunnerError> {
        match key {
            surrealdb_sql::RecordIdKeyLit::String(_)
            | surrealdb_sql::RecordIdKeyLit::Number(_)
            | surrealdb_sql::RecordIdKeyLit::Uuid(_) => Ok(()),
            _ => {
                let _ = depth;
                Err(unsupported())
            }
        }
    }
    fn literal(&mut self, literal: &Literal, depth: usize) -> Result<(), RunnerError> {
        match literal {
            Literal::RecordId(record) => {
                self.object(
                    ObjectKind::Table,
                    record.table.as_str(),
                    AccessMode::TypedRecordLink,
                )?;
                self.record_key(&record.key, depth + 1)
            }
            Literal::Array(values) | Literal::Set(values) => {
                for expr in values {
                    self.expr(expr, depth + 1)?;
                }
                Ok(())
            }
            Literal::Object(values) => {
                for entry in values {
                    self.expr(&entry.value, depth + 1)?;
                }
                Ok(())
            }
            Literal::None
            | Literal::Null
            | Literal::Bool(_)
            | Literal::Float(_)
            | Literal::Integer(_)
            | Literal::Decimal(_)
            | Literal::Duration(_)
            | Literal::String(_)
            | Literal::Datetime(_)
            | Literal::Uuid(_)
            | Literal::Regex(_)
            | Literal::Bytes(_)
            | Literal::Geometry(_) => Ok(()),
            _ => Err(unsupported()),
        }
    }
    fn fields(&mut self, fields: &surrealdb_sql::Fields, depth: usize) -> Result<(), RunnerError> {
        match fields {
            surrealdb_sql::Fields::Value(s) => {
                self.expr(&s.expr, depth + 1)?;
                if let Some(alias) = &s.alias {
                    self.idiom(alias, depth + 1)?;
                }
            }
            surrealdb_sql::Fields::Select(fields) => {
                for field in fields {
                    if let surrealdb_sql::Field::Single(s) = field {
                        self.expr(&s.expr, depth + 1)?;
                        if let Some(alias) = &s.alias {
                            self.idiom(alias, depth + 1)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
    fn output(
        &mut self,
        output: Option<&surrealdb_sql::Output>,
        depth: usize,
    ) -> Result<(), RunnerError> {
        if let Some(surrealdb_sql::Output::Fields(fields)) = output {
            self.fields(fields, depth + 1)?;
        }
        Ok(())
    }
    fn data(&mut self, data: &surrealdb_sql::Data, depth: usize) -> Result<(), RunnerError> {
        use surrealdb_sql::Data;
        match data {
            Data::EmptyExpression => Ok(()),
            Data::ContentExpression(expr)
            | Data::MergeExpression(expr)
            | Data::ReplaceExpression(expr)
            | Data::SingleExpression(expr) => self.expr(expr, depth + 1),
            Data::SetExpression(assignments) | Data::UpdateExpression(assignments) => {
                for assignment in assignments {
                    self.idiom(&assignment.place, depth + 1)?;
                    self.expr(&assignment.value, depth + 1)?;
                }
                Ok(())
            }
            Data::UnsetExpression(idioms) => {
                for idiom in idioms {
                    self.idiom(idiom, depth + 1)?;
                }
                Ok(())
            }
            _ => Err(unsupported()),
        }
    }
    fn select(
        &mut self,
        s: &surrealdb_sql::statements::SelectStatement,
        depth: usize,
    ) -> Result<(), RunnerError> {
        if s.with.is_some()
            || s.split.is_some()
            || s.group.is_some()
            || s.fetch.is_some()
            || s.explain.is_some()
            || s.tempfiles
            || s.for_update
        {
            return Err(unsupported());
        }
        for target in &s.what {
            self.target(target, false, depth + 1)?;
        }
        let row_scope = self.row_scope;
        self.row_scope = true;
        self.fields(&s.fields, depth + 1)?;
        for expr in &s.omit {
            self.expr(expr, depth + 1)?;
        }
        if let Some(c) = &s.cond {
            self.expr(&c.0, depth + 1)?;
        }
        if let Some(ordering) = &s.order {
            let surrealdb_sql::order::Ordering::Order(orders) = ordering else {
                return Err(unsupported());
            };
            for order in orders.iter() {
                self.readonly_expr(&Expr::Idiom(order.value.clone()), depth + 1)?;
            }
        }
        if let Some(limit) = &s.limit {
            self.expr(&limit.0, depth + 1)?;
        }
        if let Some(start) = &s.start {
            self.expr(&start.0, depth + 1)?;
        }
        self.expr(&s.version, depth + 1)?;
        self.expr(&s.timeout, depth + 1)?;
        self.row_scope = row_scope;
        Ok(())
    }
}

/// Catalog admission compares parsed definitions, never textual IF NOT EXISTS success.
pub(super) fn same_definition(actual: &str, expected: &str) -> Result<bool, RunnerError> {
    fn definition(sql: &str) -> Result<DefineStatement, RunnerError> {
        if sql.len() > 1_048_576 {
            return Err(RunnerError::new("catalog definition exceeds byte limit"));
        }
        let ast = surrealdb_syn::parse_with_settings(
            sql.as_bytes(),
            surrealdb_syn::ParserSettings {
                object_recursion_limit: 32,
                query_recursion_limit: 20,
                expr_recursion_limit: 64,
                ..Default::default()
            },
            async |parser, stack| {
                let ast = parser.parse_query(stack).await?;
                parser.assert_finished()?;
                Ok(ast)
            },
        )
        .map_err(|_| RunnerError::new("invalid stored catalog definition"))?;
        let mut expressions = ast.expressions.into_iter();
        let Some(TopLevelExpr::Expr(Expr::Define(statement))) = expressions.next() else {
            return Err(unsupported());
        };
        if expressions.next().is_some() {
            return Err(unsupported());
        }
        let mut statement = *statement;
        match &mut statement {
            DefineStatement::Table(s) => {
                s.kind = surrealdb_sql::statements::define::DefineKind::Default;
                s.id = None;
            }
            DefineStatement::Field(s) => {
                s.kind = surrealdb_sql::statements::define::DefineKind::Default
            }
            DefineStatement::Index(s) => {
                s.kind = surrealdb_sql::statements::define::DefineKind::Default
            }
            _ => return Err(unsupported()),
        }
        Ok(statement)
    }
    Ok(definition(actual)? == definition(expected)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_comparison_ignores_conditionals_but_preserves_admission_shape() {
        assert!(
            same_definition(
                include_str!("queries/policy/catalog_comparison_ignores_conditionals_but_preserves_admission_shape/statement_11.surql"),
                include_str!("queries/policy/catalog_comparison_ignores_conditionals_but_preserves_admission_shape/statement_12.surql")
            )
            .unwrap()
        );
        assert!(
            !same_definition(
                include_str!("queries/policy/catalog_comparison_ignores_conditionals_but_preserves_admission_shape/statement_13.surql"),
                include_str!("queries/policy/catalog_comparison_ignores_conditionals_but_preserves_admission_shape/statement_14.surql")
            )
            .unwrap()
        );
        assert!(
            !same_definition(
                include_str!("queries/policy/catalog_comparison_ignores_conditionals_but_preserves_admission_shape/statement_15.surql"),
                include_str!("queries/policy/catalog_comparison_ignores_conditionals_but_preserves_admission_shape/statement_16.surql")
            )
            .unwrap()
        );
        assert!(
            !same_definition(
                include_str!("queries/policy/catalog_comparison_ignores_conditionals_but_preserves_admission_shape/statement_17.surql"),
                include_str!("queries/policy/catalog_comparison_ignores_conditionals_but_preserves_admission_shape/statement_18.surql")
            )
            .unwrap()
        );
        assert!(
            same_definition(
                include_str!("queries/policy/catalog_comparison_ignores_conditionals_but_preserves_admission_shape/statement_19.surql"),
                include_str!("queries/policy/catalog_comparison_ignores_conditionals_but_preserves_admission_shape/statement_20.surql")
            )
            .is_err()
        );
    }
}
