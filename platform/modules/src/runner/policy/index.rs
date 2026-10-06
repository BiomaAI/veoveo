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
        self.object_fields.retain(|(t, p)| {
            t != table || !p.starts_with(path) || (preserves_object && p.len() == path.len())
        });
        Ok(())
    }
    /// Definitions and removals share proof invalidation for static field paths.
    /// A wildcard changes descendants of its maximal field-only prefix, while
    /// retaining that prefix's own object shape and unrelated sibling proofs.
    pub(super) fn invalidate_field_shape(
        &mut self,
        table: &str,
        expression: &Expr,
        preserves_object: bool,
    ) -> Result<Option<Vec<String>>, RunnerError> {
        self.static_field(expression)?;
        if let Some(path) = path(expression) {
            self.invalidate_field(table, &path, preserves_object)?;
            return Ok(Some(path));
        }
        let prefix: Vec<String> = match expression {
            Expr::Idiom(idiom) => idiom
                .0
                .iter()
                .map_while(|part| match part {
                    Part::Field(name) => Some(name.as_str().to_owned()),
                    _ => None,
                })
                .collect(),
            _ => return Err(unsupported()),
        };
        if prefix.is_empty() {
            self.clear_table_proof(table, false)?;
        } else {
            self.invalidate_field(table, &prefix, true)?;
        }
        Ok(None)
    }
    pub(super) fn field_proof(
        &mut self,
        field: &DefineFieldStatement,
        depth: usize,
    ) -> Result<(), RunnerError> {
        let table = self.name(&field.what)?.to_owned();
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
        let Some(path) = self.invalidate_field_shape(&table, &field.name, proven)? else {
            return Ok(());
        };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unresolved_root_ast_preserves_conservative_table_invalidation() {
        // Leading wildcards are not accepted by the pinned SQL grammar. Exercise
        // the defensive bookkeeping with an explicit AST instead of malformed SQL.
        let setup = ModuleSetup::builder(ModuleName::new("own").unwrap(), ModuleLayer::Kernel)
            .execution(LaneExecution::new(ExecutionImage::new("gateway").unwrap(),
                ExecutionCommand::new(vec!["module-lane".into()]).unwrap()).unwrap())
            .ownership(vec![OwnershipClaim::Table(TableName::new("own").unwrap())])
            .lane(MigrationLane::new(vec![Migration::new(MigrationVersion::new(0),
                MigrationName::new("initial").unwrap(),
                include_str!("../../../tests/queries/admission/object_path_indexes_require_closed_preceding_schema_and_preserve_dependencies/wildcard_preserves_ancestor_index.surql")
            ).unwrap()]).unwrap()).build().unwrap();
        let registry = ModuleRegistry::new(vec![setup]).unwrap();
        let selection = registry.select(vec![]).unwrap();
        let preparation = preparation::Preparation::new(&selection).unwrap();
        let body = &preparation.bodies[0];
        let selected = BTreeSet::from([body.module.name().clone()]);
        let TopLevelExpr::Expr(Expr::Define(definition)) = &body.ast.expressions[1] else {
            panic!("fixture must contain its field declaration")
        };
        let DefineStatement::Field(field) = definition.as_ref() else {
            panic!("fixture must contain its field declaration")
        };
        let mut field = field.clone();
        let Expr::Idiom(ref mut idiom) = field.name else {
            panic!("field name idiom")
        };
        idiom.0 = vec![Part::All];
        for dependent_index in [false, true] {
            let mut visitor = Visitor::new(
                &registry,
                &selected,
                &preparation,
                body.module,
                body.migration,
                body.start,
            );
            visitor
                .object_fields
                .insert(("own".into(), vec!["identity".into()]));
            visitor
                .object_fields
                .insert(("other".into(), vec!["identity".into()]));
            if dependent_index {
                visitor
                    .index_fields
                    .insert(("own".into(), "key".into()), vec![vec!["identity".into()]]);
            }
            let result = visitor.field_proof(&field, 1);
            assert_eq!(result.is_err(), dependent_index);
            assert_eq!(
                visitor
                    .object_fields
                    .contains(&("own".into(), vec!["identity".into()])),
                dependent_index
            );
            assert!(
                visitor
                    .object_fields
                    .contains(&("other".into(), vec!["identity".into()]))
            );
        }
    }
}
