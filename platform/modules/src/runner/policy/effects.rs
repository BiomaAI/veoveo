//! Leaf API effects never grant ordinary owner SQL access or argument-side mutation.
use super::*;

impl Visitor<'_> {
    pub(super) fn check_effect(&self, expr: &Expr) -> Result<(), RunnerError> {
        let mutation = matches!(
            expr,
            Expr::Create(_)
                | Expr::Update(_)
                | Expr::Delete(_)
                | Expr::Define(_)
                | Expr::Alter(_)
                | Expr::Remove(_)
        );
        if !mutation {
            return Ok(());
        }
        if self.argument_readonly {
            return Err(RunnerError::new("read-only SQL context rejects mutation"));
        }
        let Some(api) = self.api else {
            return Ok(());
        };
        let (SqlEffectProfile::OwnedUpdate(profile), Expr::Update(statement)) =
            (api.effects(), expr)
        else {
            return Err(RunnerError::new(
                "SQL API effect profile rejects this mutation",
            ));
        };
        let Some(surrealdb_sql::Data::SetExpression(assignments)) = &statement.data else {
            return Err(RunnerError::new(
                "SQL API updates require explicit SET assignments",
            ));
        };
        if assignments.is_empty() {
            return Err(unsupported());
        }
        for assignment in assignments {
            let [Part::Field(name)] = assignment.place.0.as_slice() else {
                return Err(RunnerError::new(
                    "SQL API update requires a listed top-level field",
                ));
            };
            if !profile
                .fields()
                .iter()
                .any(|field| field.as_str() == name.as_str())
            {
                return Err(RunnerError::new(
                    "SQL API update field is outside its owned update profile",
                ));
            }
        }
        Ok(())
    }
}
