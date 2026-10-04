//! Exact SurrealDB 3.3.0 AST adapter. Unsupported syntax always fails admission.
use super::RunnerError;
mod api;
mod index;
use crate::*;
use std::collections::{BTreeMap, BTreeSet};
use surrealdb_sql::ast::TopLevelExpr;
use surrealdb_sql::statements::DefineStatement;
use surrealdb_sql::{Expr, Kind, Literal, Part, Permission, Permissions};

pub(super) fn admit(selection: &ModuleSelection<'_>) -> Result<(), RunnerError> {
    for module in selection.ordered() {
        api::check_exports(module)?;
        let mut object_fields = BTreeSet::new();
        let mut index_fields = BTreeMap::new();
        for migration in module.lane().migrations() {
            let settings = surrealdb_syn::parser::ParserSettings {
                object_recursion_limit: 32,
                query_recursion_limit: 20,
                expr_recursion_limit: 64,
                ..Default::default()
            };
            let ast = surrealdb_syn::parse_with_settings(
                migration.sql().as_bytes(),
                settings,
                async |parser, stack| {
                    let ast = parser.parse_query(stack).await?;
                    parser.assert_finished()?;
                    Ok(ast)
                },
            )
            .map_err(|_| {
                RunnerError::new(format!(
                    "invalid migration syntax: {} / {}",
                    module.name(),
                    migration.filename()
                ))
            })?;
            let mut visitor = Visitor {
                registry: selection.registry(),
                module,
                nodes: 0,
                migration,
                api: None,
                parameters: BTreeMap::new(),
                objects: BTreeSet::new(),
                argument_readonly: false,
                row_scope: false,
                object_fields: std::mem::take(&mut object_fields),
                index_fields: std::mem::take(&mut index_fields),
            };
            for statement in &ast.expressions {
                let TopLevelExpr::Expr(expr) = statement else {
                    return Err(RunnerError::new(format!(
                        "{} / {}: transaction, session or privileged top-level statement rejected",
                        module.name(),
                        migration.filename()
                    )));
                };
                visitor.expr(expr, 0).map_err(|error| {
                    RunnerError::new(format!(
                        "{} / {}: {}: {}",
                        module.name(),
                        migration.filename(),
                        construct(expr),
                        error
                    ))
                })?;
            }
            object_fields = visitor.object_fields;
            index_fields = visitor.index_fields;
        }
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
    AnalyzerReference,
}
struct Visitor<'a> {
    registry: &'a ModuleRegistry,
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
impl Visitor<'_> {
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
            if matches!(mode, AccessMode::SchemaMutation | AccessMode::DataWrite) {
                return Err(RunnerError::new("SQL API is read-only"));
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
        if self.registry.depends_on(self.module.name(), owner.name()) {
            match mode {
                AccessMode::TypedRecordLink | AccessMode::AnalyzerReference => return Ok(()),
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
    fn name<'a>(&self, expr: &'a Expr) -> Result<&'a str, RunnerError> {
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
            _ => Err(unsupported()),
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
            Permission::Specific(expr) => self.expr(expr, depth + 1),
        }
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
        if (self.api.is_some() || self.argument_readonly)
            && matches!(
                expr,
                Expr::Create(_)
                    | Expr::Update(_)
                    | Expr::Delete(_)
                    | Expr::Define(_)
                    | Expr::Alter(_)
                    | Expr::Remove(_)
            )
        {
            return Err(RunnerError::new("SQL API is read-only"));
        }
        match expr {
            Expr::Literal(literal) => self.literal(literal, depth),
            Expr::Param(p) if self.api.is_some() && !self.parameters.contains_key(p.as_str()) => {
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
                if self.api.is_some() && self.parameters.contains_key(&name) {
                    return Err(RunnerError::new(
                        "SQL API LET cannot redefine a parameter or local",
                    ));
                }
                let kind = set.kind.as_ref().and_then(api::sql_type);
                self.parameters.insert(name.clone(), kind);
                self.objects.remove(&name);
                Ok(())
            }
            Expr::FunctionCall(call) => {
                // Inspect every argument even when the receiver is rejected; custom arguments cannot mutate.
                let readonly = self.argument_readonly;
                self.argument_readonly |=
                    matches!(call.receiver, surrealdb_sql::Function::Custom(_));
                for arg in &call.arguments {
                    self.expr(arg, depth + 1)?;
                }
                self.argument_readonly = readonly;
                match &call.receiver {
                    surrealdb_sql::Function::Custom(name) => {
                        return self.api_call(name, &call.arguments, depth + 1);
                    }
                    surrealdb_sql::Function::Normal(name)
                        if [
                            "array::len",
                            "array::first",
                            "type::is_object",
                            "type::is_array",
                            "string::len",
                            "string::lowercase",
                            "string::uppercase",
                            "math::abs",
                            "time::now",
                            "rand::uuid::v7",
                        ]
                        .contains(&name.as_str()) => {}
                    _ => return Err(unsupported()),
                }
                if self.api.is_some()
                    && matches!(&call.receiver, surrealdb_sql::Function::Normal(name) if ["time::now", "rand::uuid::v7"].contains(&name.as_str()))
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
                if let Some(data) = &statement.data {
                    self.data(data, depth + 1)?;
                }
                if let Some(cond) = &statement.cond {
                    self.expr(&cond.0, depth + 1)?;
                }
                self.output(statement.output.as_ref(), depth + 1)?;
                self.expr(&statement.timeout, depth + 1)
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
        if depth != 1
            && matches!(
                statement,
                DefineStatement::Table(_) | DefineStatement::Field(_) | DefineStatement::Index(_)
            )
        {
            return Err(RunnerError::new(
                "schema-shape declarations must be migration top-level statements",
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
                if s.view.is_some()
                    || !matches!(
                        s.table_type,
                        surrealdb_sql::TableType::Normal | surrealdb_sql::TableType::Any
                    )
                    || s.graphql_alias.is_some()
                    || s.graphql_deprecated.is_some()
                    || s.inline_edges_cap.is_some()
                    || s.inline_refs_cap.is_some()
                {
                    return Err(unsupported());
                }
                self.permissions(&s.permissions, depth)?;
                self.expr(&s.comment, depth)
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
                    self.kind(kind, depth)?;
                }
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
                    self.expr(expr, depth + 1)?;
                }
                match &s.default {
                    surrealdb_sql::statements::define::DefineDefault::None => {}
                    surrealdb_sql::statements::define::DefineDefault::Always(expr)
                    | surrealdb_sql::statements::define::DefineDefault::Set(expr) => {
                        self.expr(expr, depth + 1)?
                    }
                }
                self.permissions(&s.permissions, depth)?;
                self.expr(&s.comment, depth)
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
                            self.expr(&c.0, depth + 1)?;
                        }
                    }
                    _ => return Err(unsupported()),
                }
                self.expr(&s.comment, depth)
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
                self.expr(&s.comment, depth)
            }
            DefineStatement::Function(s) => {
                if let Some(api) = self
                    .module
                    .sql_apis()
                    .iter()
                    .find(|a| a.name().as_str() == format!("fn::{}", s.name))
                {
                    return self.api_definition(api, s, depth + 1);
                }
                self.object(
                    ObjectKind::Function,
                    &format!("fn::{}", s.name),
                    AccessMode::SchemaMutation,
                )?;
                if s.graphql_alias.is_some() || s.graphql_deprecated.is_some() {
                    return Err(unsupported());
                }
                for (_, kind) in &s.args {
                    self.kind(kind, depth + 1)?;
                }
                if let Some(kind) = &s.returns {
                    self.kind(kind, depth + 1)?;
                }
                for expr in &s.block.0 {
                    self.expr(expr, depth + 1)?;
                }
                self.permission(&s.permissions, depth)?;
                self.expr(&s.comment, depth)
            }
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
                self.expr(&s.when, depth + 1)?;
                for expr in &s.then {
                    self.expr(expr, depth + 1)?;
                }
                self.expr(&s.comment, depth)
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
        if depth != 1
            && matches!(
                s,
                RemoveStatement::Table(_) | RemoveStatement::Field(_) | RemoveStatement::Index(_)
            )
        {
            return Err(RunnerError::new(
                "schema-shape removals must be migration top-level statements",
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
        if depth > 64 {
            return Err(unsupported());
        }
        match kind {
            Kind::Record(tables) | Kind::Table(tables) => {
                if tables.is_empty() {
                    return Err(unsupported());
                }
                for table in tables {
                    self.object(
                        ObjectKind::Table,
                        table.as_str(),
                        AccessMode::TypedRecordLink,
                    )?;
                }
                Ok(())
            }
            Kind::Either(kinds) => {
                for kind in kinds {
                    self.kind(kind, depth + 1)?;
                }
                Ok(())
            }
            Kind::Array(kind, _) | Kind::Set(kind, _) => self.kind(kind, depth + 1),
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
            || s.order.is_some()
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
                "DEFINE TABLE history SCHEMAFULL PERMISSIONS NONE",
                "DEFINE TABLE IF NOT EXISTS history SCHEMAFULL PERMISSIONS NONE;"
            )
            .unwrap()
        );
        assert!(
            !same_definition(
                "DEFINE TABLE history SCHEMALESS PERMISSIONS NONE",
                "DEFINE TABLE history SCHEMAFULL PERMISSIONS NONE"
            )
            .unwrap()
        );
        assert!(
            !same_definition(
                "DEFINE FIELD v ON history TYPE int",
                "DEFINE FIELD v ON history TYPE string"
            )
            .unwrap()
        );
        assert!(
            !same_definition(
                "DEFINE INDEX by_v ON history FIELDS v",
                "DEFINE INDEX by_v ON history FIELDS v UNIQUE"
            )
            .unwrap()
        );
        assert!(
            same_definition(
                "DEFINE TABLE history; DELETE history;",
                "DEFINE TABLE history"
            )
            .is_err()
        );
    }
}
