//! Narrow leaf API qualification and positive-branch object provenance.
use super::*;
use surrealdb_sql::statements::{DefineFunctionStatement, IfelseStatement};

pub(super) fn sql_type(kind: &Kind) -> Option<SqlType> {
    Some(match kind {
        Kind::Bool => SqlType::Bool,
        Kind::String => SqlType::String,
        Kind::Datetime => SqlType::Datetime,
        Kind::Object => SqlType::Object,
        Kind::Record(tables) if tables.len() == 1 => {
            SqlType::Record(TableName::new(tables[0].as_str()).ok()?)
        }
        Kind::Array(kind, None) => SqlType::Array(Box::new(sql_type(kind)?)),
        Kind::Either(kinds) if kinds.len() == 2 => {
            let value = kinds.iter().find(|k| !matches!(k, Kind::None))?;
            if !kinds.iter().any(|k| matches!(k, Kind::None)) {
                return None;
            }
            SqlType::Option(Box::new(sql_type(value)?))
        }
        _ => return None,
    })
}
pub(super) fn definition(source: &str) -> Result<DefineFunctionStatement, RunnerError> {
    let ast = surrealdb_syn::parse_with_settings(
        source.as_bytes(),
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
    .map_err(|_| RunnerError::new("invalid declared SQL API definition"))?;
    if ast.expressions.len() != 1 {
        return Err(unsupported());
    }
    match ast.expressions.into_iter().next().unwrap() {
        TopLevelExpr::Expr(Expr::Define(s)) => match *s {
            DefineStatement::Function(s) => Ok(s),
            _ => Err(unsupported()),
        },
        _ => Err(unsupported()),
    }
}
impl<'a> Visitor<'a> {
    pub(super) fn api_definition(
        &mut self,
        api: &'a KernelSqlApi,
        statement: &DefineFunctionStatement,
        depth: usize,
    ) -> Result<(), RunnerError> {
        if self.migration.version() != api.introduced()
            || self.preparation.api(api.name()) != Some(statement)
            || statement.args.len() != api.signature().parameters().len()
            || statement.args.iter().zip(api.signature().parameters()).any(
                |((name, kind), expected)| {
                    name.as_str() != expected.name()
                        || sql_type(kind).as_ref() != Some(expected.kind())
                },
            )
            || statement.returns.as_ref().and_then(sql_type).as_ref()
                != Some(api.signature().returns())
        {
            return Err(RunnerError::new(
                "SQL API definition, introduction version or signature differs from declaration",
            ));
        }
        self.object(
            ObjectKind::Function,
            api.name().as_str(),
            AccessMode::SchemaMutation,
        )?;
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
        self.parameters = api
            .signature()
            .parameters()
            .iter()
            .map(|p| (p.name().into(), Some(p.kind().clone())))
            .collect();
        self.api = Some(api);
        self.objects = api
            .signature()
            .parameters()
            .iter()
            .filter(|p| p.kind() == &SqlType::Object)
            .map(|p| p.name().into())
            .collect();
        for expr in &statement.block.0 {
            self.expr(expr, depth + 1)?;
        }
        self.permission(&statement.permissions, depth + 1)?;
        self.readonly_expr(&statement.comment, depth + 1)?;
        self.api = None;
        self.parameters = parameters;
        self.objects = objects;
        Ok(())
    }
    pub(super) fn guarded_if(
        &mut self,
        statement: &IfelseStatement,
        depth: usize,
    ) -> Result<(), RunnerError> {
        // Each branch starts from the same incoming proof. ELSE and sibling branches never inherit it.
        for (condition, body) in &statement.exprs {
            self.expr(condition, depth + 1)?;
            let parameters = self.parameters.clone();
            let objects = self.objects.clone();
            if let Expr::FunctionCall(call) = condition
                && matches!(&call.receiver, surrealdb_sql::Function::Normal(n) if n == "type::is_object")
                && let [Expr::Param(p)] = call.arguments.as_slice()
            {
                self.objects.insert(p.as_str().into());
            }
            self.expr(body, depth + 1)?;
            self.parameters = parameters;
            self.objects = objects;
        }
        if let Some(body) = &statement.close {
            self.expr(body, depth + 1)?;
        }
        Ok(())
    }
    pub(super) fn api_call(
        &mut self,
        name: &str,
        arguments: &[Expr],
        _depth: usize,
    ) -> Result<(), RunnerError> {
        // Leaf exports never call custom functions, including themselves or other declared exports.
        if self.api.is_some() {
            return Err(RunnerError::new(
                "SQL API must be a leaf; custom calls rejected",
            ));
        }
        let name = FunctionName::new(format!("fn::{name}")).map_err(|_| unsupported())?;
        let owner = self
            .registry
            .owner_of_function(&name)
            .ok_or_else(unsupported)?;
        let Some(api) = owner.sql_apis().iter().find(|api| api.name() == &name) else {
            return self.private_call(&name, owner, arguments, _depth);
        };
        if self.argument_readonly && !matches!(api.effects(), SqlEffectProfile::ReadOnly) {
            return Err(RunnerError::new(
                "read-only SQL context cannot call a mutating API",
            ));
        }
        self.preparation
            .function(&name, self.position, self.deferred)?;
        let allowed = if owner.name() == self.module.name() {
            self.migration.version() >= api.introduced()
        } else {
            self.registry.depends_on(self.module.name(), owner.name())
                && self.migration.requires().iter().any(|r| {
                    r.module() == owner.name() && r.minimum().is_some_and(|v| v >= api.introduced())
                })
        };
        if !allowed {
            return Err(RunnerError::new(
                "SQL API call lacks owner dependency and concrete migration minimum",
            ));
        }
        if arguments.len() != api.signature().parameters().len()
            || arguments
                .iter()
                .zip(api.signature().parameters())
                .any(|(arg, p)| !self.argument(arg, p.kind()))
        {
            return Err(RunnerError::new(
                "SQL API call argument signature is not proven",
            ));
        }
        Ok(())
    }
    fn argument(&self, expr: &Expr, kind: &SqlType) -> bool {
        if let Expr::Param(p) = expr {
            return self.parameters.get(p.as_str()).and_then(Option::as_ref) == Some(kind);
        }
        if let Expr::Prefix {
            op: surrealdb_sql::PrefixOperator::Cast(k),
            ..
        } = expr
        {
            return sql_type(k).as_ref() == Some(kind);
        }
        match (expr, kind) {
            (Expr::Literal(Literal::Bool(_)), SqlType::Bool)
            | (Expr::Literal(Literal::String(_)), SqlType::String)
            | (Expr::Literal(Literal::Datetime(_)), SqlType::Datetime)
            | (Expr::Literal(Literal::Object(_)), SqlType::Object) => true,
            (Expr::Literal(Literal::RecordId(r)), SqlType::Record(t)) => {
                r.table.as_str() == t.as_str()
            }
            (Expr::Literal(Literal::None), SqlType::Option(_)) => true,
            (expr, SqlType::Option(inner)) => self.argument(expr, inner),
            (Expr::Literal(Literal::Array(values)), SqlType::Array(inner)) => {
                values.iter().all(|v| self.argument(v, inner))
            }
            _ => false,
        }
    }
}
