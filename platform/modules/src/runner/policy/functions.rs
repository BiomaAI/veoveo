//! Complete private callee inspection and revalidation of surviving deferred definitions.
use super::*;
use surrealdb_sql::statements::{DefineFunctionStatement, RemoveStatement};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum RootKey {
    Table(TableName),
    Field(TableName, Vec<String>),
    Event(TableName, String),
    Index(TableName, String),
    Function(FunctionName),
}
struct Root<'a> {
    module: &'a ModuleSetup,
    migration: &'a Migration,
    expr: Expr,
    readonly: bool,
    object_fields: BTreeSet<(String, Vec<String>)>,
}
#[derive(Default)]
pub(super) struct DeferredRoots<'a> {
    roots: BTreeMap<RootKey, Root<'a>>,
}
fn static_name(expr: &Expr) -> Result<&str, RunnerError> {
    match expr {
        Expr::Table(name) => Ok(name.as_str()),
        Expr::Idiom(path) => match path.0.as_slice() {
            [Part::Field(name)] => Ok(name.as_str()),
            _ => Err(unsupported()),
        },
        _ => Err(unsupported()),
    }
}
fn table(expr: &Expr) -> Result<TableName, RunnerError> {
    TableName::new(static_name(expr)?).map_err(|_| unsupported())
}
fn field(expr: &Expr) -> Result<Vec<String>, RunnerError> {
    let Expr::Idiom(path) = expr else {
        return Err(unsupported());
    };
    path.0
        .iter()
        .map(|part| match part {
            Part::Field(name) => Ok(name.as_str().into()),
            Part::All => Ok("*".into()),
            _ => Err(unsupported()),
        })
        .collect()
}
impl<'a> DeferredRoots<'a> {
    pub fn observe(&mut self, visitor: &Visitor<'a>, expr: &Expr) -> Result<bool, RunnerError> {
        let module = visitor.module;
        let migration = visitor.migration;
        match expr {
            Expr::Define(statement) => {
                let mut statement = statement.as_ref().clone();
                let key = match &mut statement {
                    DefineStatement::Table(s) => {
                        s.comment = Expr::Literal(Literal::None);
                        RootKey::Table(table(&s.name)?)
                    }
                    DefineStatement::Field(s) => {
                        s.comment = Expr::Literal(Literal::None);
                        RootKey::Field(table(&s.what)?, field(&s.name)?)
                    }
                    DefineStatement::Event(s) => {
                        s.comment = Expr::Literal(Literal::None);
                        RootKey::Event(table(&s.target_table)?, static_name(&s.name)?.into())
                    }
                    DefineStatement::Index(s) => {
                        s.comment = Expr::Literal(Literal::None);
                        RootKey::Index(table(&s.what)?, static_name(&s.name)?.into())
                    }
                    DefineStatement::Function(s) => {
                        if module
                            .sql_apis()
                            .iter()
                            .any(|api| api.name().as_str() == format!("fn::{}", s.name))
                        {
                            return Ok(true);
                        }
                        s.comment = Expr::Literal(Literal::None);
                        RootKey::Function(
                            FunctionName::new(format!("fn::{}", s.name))
                                .map_err(|_| unsupported())?,
                        )
                    }
                    _ => return Ok(false),
                };
                // Comments execute at definition time; only stored bodies/predicates survive.
                let changed = matches!(statement, DefineStatement::Function(_));
                self.roots.insert(
                    key,
                    Root {
                        module,
                        migration,
                        readonly: Self::is_readonly(visitor, &statement),
                        object_fields: Self::index_fields(&statement),
                        expr: Expr::Define(Box::new(statement)),
                    },
                );
                Ok(changed)
            }
            Expr::Remove(statement) => {
                match statement.as_ref() {
                    RemoveStatement::Table(s) => {
                        let removed = table(&s.name)?;
                        self.roots.retain(|key, _| match key {
                            RootKey::Table(t)
                            | RootKey::Field(t, _)
                            | RootKey::Event(t, _)
                            | RootKey::Index(t, _) => t != &removed,
                            RootKey::Function(_) => true,
                        });
                    }
                    RemoveStatement::Field(s) => {
                        self.roots
                            .remove(&RootKey::Field(table(&s.what)?, field(&s.name)?));
                    }
                    RemoveStatement::Event(s) => {
                        self.roots.remove(&RootKey::Event(
                            table(&s.what)?,
                            static_name(&s.name)?.into(),
                        ));
                    }
                    RemoveStatement::Index(s) => {
                        self.roots.remove(&RootKey::Index(
                            table(&s.what)?,
                            static_name(&s.name)?.into(),
                        ));
                    }
                    RemoveStatement::Function(s) => {
                        self.roots.remove(&RootKey::Function(
                            FunctionName::new(format!("fn::{}", s.name))
                                .map_err(|_| unsupported())?,
                        ));
                        return Ok(true);
                    }
                    _ => {}
                }
                Ok(false)
            }
            Expr::Alter(statement) => {
                let surrealdb_sql::statements::AlterStatement::Table(alter) = statement.as_ref()
                else {
                    return Ok(false);
                };
                let Some(permissions) = &alter.permissions else {
                    return Ok(false);
                };
                let key = RootKey::Table(table(&alter.name)?);
                if let Some(root) = self.roots.get_mut(&key) {
                    match &mut root.expr {
                        Expr::Define(statement) => match statement.as_mut() {
                            DefineStatement::Table(table) => {
                                table.permissions = permissions.clone()
                            }
                            _ => unreachable!("table root has a table definition"),
                        },
                        Expr::Alter(statement) => match statement.as_mut() {
                            surrealdb_sql::statements::AlterStatement::Table(table) => {
                                table.permissions = Some(permissions.clone())
                            }
                            _ => unreachable!("table root has a table alteration"),
                        },
                        _ => unreachable!("table root has a schema statement"),
                    }
                } else {
                    self.roots.insert(
                        key,
                        Root {
                            module,
                            migration,
                            expr: expr.clone(),
                            readonly: true,
                            object_fields: BTreeSet::new(),
                        },
                    );
                }
                Ok(false)
            }
            _ => Ok(false),
        }
    }
    fn index_fields(statement: &DefineStatement) -> BTreeSet<(String, Vec<String>)> {
        let DefineStatement::Index(index) = statement else {
            return BTreeSet::new();
        };
        let Ok(table) = static_name(&index.what) else {
            return BTreeSet::new();
        };
        index
            .cols
            .iter()
            .filter_map(super::index::path)
            .flat_map(|path| {
                (1..path.len()).map(move |length| (table.into(), path[..length].to_vec()))
            })
            .collect()
    }
    fn is_readonly(visitor: &Visitor<'a>, statement: &DefineStatement) -> bool {
        let mut probe = Visitor::new(
            visitor.registry,
            visitor.selected_modules,
            visitor.preparation,
            visitor.module,
            visitor.migration,
            visitor.position,
        );
        probe.argument_readonly = true;
        match statement {
            DefineStatement::Function(function) => {
                let Ok(name) = FunctionName::new(format!("fn::{}", function.name)) else {
                    return false;
                };
                probe.private_body(&name, function, true, 2).is_ok()
            }
            DefineStatement::Event(event) => {
                probe.deferred = true;
                for name in ["before", "after"] {
                    probe.parameters.insert(name.into(), Some(SqlType::Object));
                    probe.objects.insert(name.into());
                }
                event.then.iter().all(|expr| probe.expr(expr, 2).is_ok())
            }
            _ => true,
        }
    }
    pub fn validate(
        &self,
        registry: &'a ModuleRegistry,
        selected: &'a BTreeSet<ModuleName>,
        preparation: &'a preparation::Preparation<'a>,
        position: usize,
    ) -> Result<(), RunnerError> {
        for root in self.roots.values() {
            let mut visitor = Visitor::new(
                registry,
                selected,
                preparation,
                root.module,
                root.migration,
                position,
            );
            visitor.object_fields = root.object_fields.clone();
            let result = (|| match (&root.expr, root.readonly) {
                (Expr::Define(statement), true) => match statement.as_ref() {
                    DefineStatement::Function(function) => {
                        visitor.argument_readonly = true;
                        let name = FunctionName::new(format!("fn::{}", function.name))
                            .map_err(|_| unsupported())?;
                        visitor.private_body(&name, function, true, 2)?;
                        visitor.permission(&function.permissions, 2)
                    }
                    DefineStatement::Event(event) => {
                        visitor.argument_readonly = true;
                        visitor.deferred = true;
                        for name in ["before", "after"] {
                            visitor
                                .parameters
                                .insert(name.into(), Some(SqlType::Object));
                            visitor.objects.insert(name.into());
                        }
                        visitor.readonly_expr(&event.when, 2)?;
                        event.then.iter().try_for_each(|expr| visitor.expr(expr, 2))
                    }
                    _ => visitor.expr(&root.expr, 0),
                },
                _ => visitor.expr(&root.expr, 0),
            })();
            result.map_err(|error| {
                RunnerError::new(format!(
                    "surviving deferred definition in {} / {}: {error}",
                    root.module.name(),
                    root.migration.filename()
                ))
            })?;
        }
        Ok(())
    }
}
impl<'a> Visitor<'a> {
    pub(super) fn function_definition(
        &mut self,
        statement: &DefineFunctionStatement,
        depth: usize,
    ) -> Result<(), RunnerError> {
        if depth != 2 {
            return Err(RunnerError::new(
                "function definitions must be migration top-level statements",
            ));
        }
        if matches!(
            statement.kind,
            surrealdb_sql::statements::define::DefineKind::IfNotExists
        ) {
            return Err(RunnerError::new(
                "conditional function definitions cannot prove stored body identity",
            ));
        }
        let name =
            FunctionName::new(format!("fn::{}", statement.name)).map_err(|_| unsupported())?;
        self.object(
            ObjectKind::Function,
            name.as_str(),
            AccessMode::SchemaMutation,
        )?;
        if let Some(api) = self
            .module
            .sql_apis()
            .iter()
            .find(|api| api.name() == &name)
        {
            return self.api_definition(api, statement, depth);
        }
        if name.as_str().starts_with("fn::kernel::") {
            return Err(RunnerError::new(
                "kernel SQL functions require an explicit versioned API declaration",
            ));
        }
        self.private_body(&name, statement, true, depth)?;
        self.permission(&statement.permissions, depth)?;
        self.readonly_expr(&statement.comment, depth)
    }
    fn private_body(
        &mut self,
        name: &FunctionName,
        statement: &DefineFunctionStatement,
        deferred: bool,
        depth: usize,
    ) -> Result<(), RunnerError> {
        if statement.graphql_alias.is_some() || statement.graphql_deprecated.is_some() {
            return Err(unsupported());
        }
        for (_, kind) in &statement.args {
            self.kind(kind, depth + 1)?;
        }
        if let Some(kind) = &statement.returns {
            self.kind(kind, depth + 1)?;
        }
        let parameters = std::mem::take(&mut self.parameters);
        let objects = std::mem::take(&mut self.objects);
        let function_body = self.function_body;
        let prior_deferred = self.deferred;
        self.function_body = true;
        self.deferred = deferred;
        self.parameters = statement
            .args
            .iter()
            .map(|(name, kind)| (name.as_str().into(), api::sql_type(kind)))
            .collect();
        self.objects = statement
            .args
            .iter()
            .filter(|(_, kind)| matches!(kind, Kind::Object))
            .map(|(name, _)| name.as_str().into())
            .collect();
        if self.preparation.analysis.borrow().stack.contains(name) {
            return Err(RunnerError::new(format!(
                "recursive function cycle at {name}"
            )));
        }
        self.preparation
            .analysis
            .borrow_mut()
            .stack
            .push(name.clone());
        let result = statement
            .block
            .0
            .iter()
            .try_for_each(|expr| self.expr(expr, depth + 1));
        self.preparation.analysis.borrow_mut().stack.pop();
        self.parameters = parameters;
        self.objects = objects;
        self.function_body = function_body;
        self.deferred = prior_deferred;
        result
    }
    pub(super) fn private_call(
        &mut self,
        name: &FunctionName,
        owner: &'a ModuleSetup,
        arguments: &[Expr],
        depth: usize,
    ) -> Result<(), RunnerError> {
        let version = self
            .preparation
            .function(name, self.position, self.deferred)?;
        let same_owner = owner.name() == self.module.name();
        let readonly = self.argument_readonly || !same_owner;
        if !same_owner
            && !(self.module.layer() == ModuleLayer::Optional
                && owner.layer() == ModuleLayer::Optional
                && self.registry.depends_on(self.module.name(), owner.name())
                && self.migration.requires().iter().any(|requirement| {
                    requirement.module() == owner.name()
                        && requirement
                            .minimum()
                            .is_some_and(|minimum| minimum >= version.introduced)
                }))
        {
            return Err(RunnerError::new(
                "private function call requires an optional owner dependency and concrete introducing minimum",
            ));
        }
        let statement = version
            .statement
            .as_ref()
            .expect("function lookup admits a definition");
        if arguments.len() != statement.args.len() {
            return Err(RunnerError::new(
                "private function argument count differs from its native signature",
            ));
        }
        if self.preparation.analysis.borrow().stack.contains(name) {
            return Err(RunnerError::new(format!(
                "recursive function cycle at {name}"
            )));
        }
        let key = (
            version.position,
            self.position,
            readonly,
            self.deferred,
            depth,
        );
        let cached = self.preparation.analysis.borrow().memo.get(&key).copied();
        let nodes = if let Some(nodes) = cached {
            nodes
        } else {
            let mut visitor = Visitor::new(
                self.registry,
                self.selected_modules,
                self.preparation,
                version.module,
                version.migration,
                self.position,
            );
            visitor.argument_readonly = readonly;
            visitor.private_body(name, statement, self.deferred, depth)?;
            let nodes = visitor.nodes;
            self.preparation
                .analysis
                .borrow_mut()
                .memo
                .insert(key, nodes);
            nodes
        };
        self.nodes = self.nodes.checked_add(nodes).ok_or_else(unsupported)?;
        if self.nodes > 100_000 {
            return Err(RunnerError::new("SQL admission complexity budget exceeded"));
        }
        Ok(())
    }
}
