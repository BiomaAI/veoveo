use secrecy::{ExposeSecret, SecretString};

use crate::{PlatformStore, StoreError, config::validate_name};

impl PlatformStore {
    /// Create or rotate the installation's database-scoped runtime account.
    pub async fn replace_database_editor(
        &self,
        username: &str,
        password: &SecretString,
    ) -> Result<(), StoreError> {
        self.require_root("database runtime user rotation")?;
        validate_name("runtime username", username)?;
        if password.expose_secret().is_empty() {
            return Err(crate::StoreConfigError::EmptyPassword.into());
        }

        // DEFINE USER requires a strand literal for PASSWORD; SurrealQL does
        // not accept a bound parameter at this grammar position. JSON string
        // escaping is also valid SurrealQL strand escaping.
        let password_literal = serde_json::to_string(password.expose_secret())
            .expect("serializing a string cannot fail");
        let statement = format!(
            "DEFINE USER OVERWRITE `{username}` ON DATABASE PASSWORD {password_literal} ROLES EDITOR;"
        );
        self.db
            .query(statement)
            .await
            .map_err(|_| StoreError::AdministrationFailed {
                operation: "database runtime user rotation",
            })?
            .check()
            .map_err(|_| StoreError::AdministrationFailed {
                operation: "database runtime user rotation",
            })?;
        Ok(())
    }

    /// Verify that the selected namespace/database accepts queries.
    pub async fn healthcheck(&self) -> Result<(), StoreError> {
        self.db.query("RETURN true;").await?.check()?;
        Ok(())
    }
}
