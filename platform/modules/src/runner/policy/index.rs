//! Schema-proven object paths for index expressions, separate from executable field access.
use super::*;
use surrealdb_sql::statements::define::{DefineFieldStatement, DefineIndexStatement, DefineKind};

pub(super) fn path(expr: &Expr) -> Option<Vec<String>> {
    let Expr::Idiom(i) = expr else {
        return None;
    };
    i.0.iter()
        .map(|p| match p {
            Part::Field(f) => Some(f.as_str().into()),
            _ => None,
        })
        .collect()
}
impl Visitor<'_> {
    pub(super) fn clear_table_proof(
        &mut self,
        table: &str,
        removed: bool,
    ) -> Result<(), RunnerError> {
        if !removed && self.index_fields.keys().any(|(t, _)| t == table) {
            return Err(RunnerError::new(
                "remove object-path indexes before redefining their table",
            ));
        }
        self.object_fields.retain(|(t, _)| t != table);
        if removed {
            self.index_fields.retain(|(t, _), _| t != table);
        }
        Ok(())
    }
    pub(super) fn invalidate_field(
        &mut self,
        table: &str,
        path: &[String],
        preserves_object: bool,
    ) -> Result<(), RunnerError> {
        if self.index_fields.iter().any(|((t, _), fields)| {
            t == table
                && fields
                    .iter()
                    .any(|p| p.starts_with(path) && (!preserves_object || p.len() > path.len()))
        }) {
            return Err(RunnerError::new(
                "remove dependent object-path indexes before changing their field shape",
            ));
        }
        self.object_fields
            .retain(|(t, p)| t != table || !p.starts_with(path));
        Ok(())
    }
    pub(super) fn field_proof(
        &mut self,
        field: &DefineFieldStatement,
        depth: usize,
    ) -> Result<(), RunnerError> {
        let table = self.name(&field.what)?.to_owned();
        let Some(path) = path(&field.name) else {
            // Wildcards may change descendants; they cannot preserve another field's proof.
            return self.clear_table_proof(&table, false);
        };
        let object = field
            .field_kind
            .as_ref()
            .and_then(api::sql_type)
            .is_some_and(|k| {
                matches!(k, SqlType::Object)
                    || matches!(k, SqlType::Option(inner) if *inner == SqlType::Object)
            });
        // IF NOT EXISTS cannot certify an existing shape. Only unconditional top-level definitions grant proof.
        let proven = depth == 1 && field.kind != DefineKind::IfNotExists && object;
        self.invalidate_field(&table, &path, proven)?;
        if proven {
            self.object_fields.insert((table, path));
        }
        Ok(())
    }
    pub(super) fn index_proof(
        &mut self,
        index: &DefineIndexStatement,
        depth: usize,
    ) -> Result<(), RunnerError> {
        let table = self.name(&index.what)?.to_owned();
        let name = self.name(&index.name)?.to_owned();
        let mut required = Vec::new();
        for expression in &index.cols {
            if let Some(path) = path(expression) {
                for length in 1..path.len() {
                    let prefix = path[..length].to_vec();
                    if !self
                        .object_fields
                        .contains(&(table.clone(), prefix.clone()))
                    {
                        return Err(RunnerError::new(
                            "index path requires preceding object/option<object> definitions for every intermediate",
                        ));
                    }
                    required.push(prefix);
                }
            } else {
                // Computed columns retain ordinary child inspection; no path exception reaches them.
                let readonly = self.argument_readonly;
                self.argument_readonly = true;
                self.expr(expression, depth + 1)?;
                self.argument_readonly = readonly;
            }
        }
        if !required.is_empty() {
            let entry = self.index_fields.entry((table, name)).or_default();
            if index.kind != DefineKind::IfNotExists {
                entry.clear();
            }
            entry.extend(required);
        }
        Ok(())
    }
}
