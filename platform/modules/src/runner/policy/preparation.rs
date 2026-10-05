//! Parsed owner bodies and exact ordered function versions; private to preparation.
use super::*;
use std::cell::RefCell;
use surrealdb_sql::ast::Ast;
use surrealdb_sql::statements::{DefineFunctionStatement, RemoveStatement};

pub(super) struct Body<'a> {
    pub module: &'a ModuleSetup,
    pub migration: &'a Migration,
    pub ast: Ast,
    pub start: usize,
}
pub(super) struct FunctionVersion<'a> {
    pub module: &'a ModuleSetup,
    pub migration: &'a Migration,
    pub position: usize,
    pub introduced: MigrationVersion,
    pub statement: Option<DefineFunctionStatement>,
}
#[derive(Default)]
pub(super) struct Analysis {
    pub stack: Vec<FunctionName>,
    pub memo: BTreeMap<(usize, usize, bool, bool, usize), usize>,
}
pub(super) struct Preparation<'a> {
    pub bodies: Vec<Body<'a>>,
    functions: BTreeMap<FunctionName, Vec<FunctionVersion<'a>>>,
    apis: BTreeMap<FunctionName, DefineFunctionStatement>,
    pub analysis: RefCell<Analysis>,
}
impl<'a> Preparation<'a> {
    pub fn new(selection: &ModuleSelection<'a>) -> Result<Self, RunnerError> {
        let mut bodies = Vec::new();
        let mut functions: BTreeMap<FunctionName, Vec<FunctionVersion<'a>>> = BTreeMap::new();
        let mut position = 0;
        let mut installed = BTreeMap::new();
        for module in selection.ordered() {
            for migration in module.lane().migrations() {
                let ast = parse(migration.sql()).map_err(|_| {
                    RunnerError::new(format!(
                        "invalid migration syntax: {} / {}",
                        module.name(),
                        migration.filename()
                    ))
                })?;
                let start = position;
                for statement in &ast.expressions {
                    let TopLevelExpr::Expr(expr) = statement else {
                        return Err(RunnerError::new(
                            "transaction, session or privileged top-level statement rejected",
                        ));
                    };
                    match expr {
                        Expr::Define(statement) => {
                            if let DefineStatement::Function(function) = statement.as_ref() {
                                let name = FunctionName::new(format!("fn::{}", function.name))
                                    .map_err(|_| unsupported())?;
                                if installed.contains_key(&name)
                                    && matches!(
                                        function.kind,
                                        surrealdb_sql::statements::define::DefineKind::Default
                                    )
                                {
                                    return Err(RunnerError::new(format!(
                                        "function {name} redefinition requires explicit OVERWRITE"
                                    )));
                                }
                                let introduced =
                                    *installed.entry(name.clone()).or_insert(migration.version());
                                functions.entry(name).or_default().push(FunctionVersion {
                                    module,
                                    migration,
                                    position,
                                    introduced,
                                    statement: Some(function.clone()),
                                });
                            }
                        }
                        Expr::Remove(statement) => {
                            if let RemoveStatement::Function(function) = statement.as_ref() {
                                let name = FunctionName::new(format!("fn::{}", function.name))
                                    .map_err(|_| unsupported())?;
                                installed.remove(&name);
                                functions.entry(name).or_default().push(FunctionVersion {
                                    module,
                                    migration,
                                    position,
                                    introduced: migration.version(),
                                    statement: None,
                                });
                            }
                        }
                        _ => {}
                    }
                    position += 1;
                }
                bodies.push(Body {
                    module,
                    migration,
                    ast,
                    start,
                });
            }
        }
        let mut apis = BTreeMap::new();
        for module in selection.ordered() {
            for api in module.sql_apis() {
                let definition = super::api::definition(api.definition())?;
                let versions = functions
                    .get(api.name())
                    .ok_or_else(|| RunnerError::new("SQL API definition missing"))?;
                if api.name().as_str() != format!("fn::{}", definition.name)
                    || versions.len() != 1
                    || versions[0].migration.version() != api.introduced()
                    || versions[0].statement.as_ref() != Some(&definition)
                {
                    return Err(RunnerError::new(
                        "SQL API definition missing, duplicated, removed or changed",
                    ));
                }
                apis.insert(api.name().clone(), definition);
            }
        }
        Ok(Self {
            bodies,
            functions,
            apis,
            analysis: RefCell::default(),
        })
    }
    pub fn api(&self, name: &FunctionName) -> Option<&DefineFunctionStatement> {
        self.apis.get(name)
    }
    pub fn function(
        &self,
        name: &FunctionName,
        position: usize,
        deferred: bool,
    ) -> Result<&FunctionVersion<'a>, RunnerError> {
        let versions = self
            .functions
            .get(name)
            .ok_or_else(|| RunnerError::new(format!("unresolved function {name}")))?;
        let version = versions
            .iter()
            .rev()
            .find(|version| version.position <= position)
            .or_else(|| deferred.then(|| versions.first()).flatten())
            .ok_or_else(|| {
                RunnerError::new(format!("function {name} is unavailable at this statement"))
            })?;
        if version.statement.is_none() {
            return Err(RunnerError::new(format!(
                "removed function {name} still has a caller"
            )));
        }
        Ok(version)
    }
}
fn parse(sql: &str) -> Result<Ast, RunnerError> {
    surrealdb_syn::parse_with_settings(
        sql.as_bytes(),
        surrealdb_syn::parser::ParserSettings {
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
    .map_err(|_| unsupported())
}
