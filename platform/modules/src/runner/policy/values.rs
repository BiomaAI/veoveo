//! Native scalar methods, field-row context and closure children with no record traversal.
use super::*;
impl Visitor<'_> {
    pub(super) fn value_idiom(
        &mut self,
        idiom: &surrealdb_sql::Idiom,
        depth: usize,
    ) -> Result<(), RunnerError> {
        if let [Part::Start(Expr::Param(parameter)), Part::Field(_)] = idiom.0.as_slice()
            && (self.objects.contains(parameter.as_str())
                || (parameter.as_str() == "this" && self.this_table.is_some()))
        {
            return self.expr(&Expr::Param(parameter.clone()), depth + 1);
        }
        if let [Part::Start(receiver), Part::Method(name, arguments)] = idiom.0.as_slice() {
            let kind = match receiver {
                Expr::Param(parameter) if parameter.as_str() == "value" => {
                    self.field_value.as_ref()
                }
                _ => None,
            };
            let allowed = matches!(
                (kind, name.as_str(), arguments.as_slice()),
                (Some(Kind::String), "len", [])
                    | (
                        Some(Kind::Array(_, _) | Kind::Set(_, _)),
                        "len" | "distinct",
                        []
                    )
                    | (
                        Some(Kind::Array(_, _) | Kind::Set(_, _)),
                        "all",
                        [Expr::Closure(_)]
                    )
            );
            if !allowed {
                return Err(unsupported());
            }
            self.expr(receiver, depth + 1)?;
            for argument in arguments {
                self.readonly_expr(argument, depth + 1)?;
            }
            return Ok(());
        }
        Err(unsupported())
    }
    pub(super) fn closure(
        &mut self,
        closure: &surrealdb_sql::Closure,
        depth: usize,
    ) -> Result<(), RunnerError> {
        let parameters = self.parameters.clone();
        let objects = self.objects.clone();
        for (name, kind) in &closure.args {
            self.kind(kind, depth + 1)?;
            if self.parameters.contains_key(name.as_str()) {
                return Err(RunnerError::new("closure cannot shadow an admitted local"));
            }
            self.parameters
                .insert(name.as_str().into(), api::sql_type(kind));
            if matches!(kind, Kind::Object) {
                self.objects.insert(name.as_str().into());
            }
        }
        if let Some(kind) = &closure.returns {
            self.kind(kind, depth + 1)?;
        }
        let result = self.readonly_expr(&closure.body, depth + 1);
        self.parameters = parameters;
        self.objects = objects;
        result
    }
    pub(super) fn view(
        &mut self,
        view: &surrealdb_sql::View,
        depth: usize,
    ) -> Result<(), RunnerError> {
        for table in &view.what {
            self.object(ObjectKind::Table, table.as_str(), AccessMode::DataRead)?;
        }
        let row_scope = self.row_scope;
        let readonly = self.argument_readonly;
        self.row_scope = true;
        self.argument_readonly = true;
        self.fields(&view.expr, depth + 1)?;
        if let Some(condition) = &view.cond {
            self.expr(&condition.0, depth + 1)?;
        }
        if let Some(groups) = &view.group {
            for group in &groups.0 {
                self.idiom(&group.0, depth + 1)?;
            }
        }
        self.row_scope = row_scope;
        self.argument_readonly = readonly;
        Ok(())
    }
}
