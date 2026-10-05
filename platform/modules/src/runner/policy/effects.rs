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
        let (fields, data) = match (api.effects(), expr) {
            (SqlEffectProfile::OwnedUpdate(profile), Expr::Update(statement)) => {
                (profile.fields(), &statement.data)
            }
            (SqlEffectProfile::OwnedCreate(profile), Expr::Create(statement)) => {
                if !statement.only
                    || statement.what.len() != 1
                    || !matches!(
                        statement.what.first(),
                        Some(Expr::Param(_)) | Some(Expr::Literal(Literal::RecordId(_)))
                    )
                {
                    return Err(RunnerError::new(
                        "SQL API create requires ONLY one typed owned record target",
                    ));
                }
                (profile.fields(), &statement.data)
            }
            _ => {
                return Err(RunnerError::new(
                    "SQL API effect profile rejects this mutation",
                ));
            }
        };
        let Some(surrealdb_sql::Data::SetExpression(assignments)) = data else {
            return Err(RunnerError::new(
                "SQL API mutations require explicit SET assignments",
            ));
        };
        if assignments.is_empty() {
            return Err(unsupported());
        }
        for assignment in assignments {
            let [Part::Field(name)] = assignment.place.0.as_slice() else {
                return Err(RunnerError::new(
                    "SQL API mutation requires a listed top-level field",
                ));
            };
            if !fields.iter().any(|field| field.as_str() == name.as_str()) {
                return Err(RunnerError::new(
                    "SQL API mutation field is outside its owned effect profile",
                ));
            }
        }
        Ok(())
    }
}
