use crate::{
    hosting::server::Handler,
    persistence::sql::{Migration, SqlConfig},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Deployment {
    pub name: String,
    #[serde(rename = "path-prefix")]
    pub path_prefix: String,
    #[serde(default)]
    pub kv: bool,
    #[serde(default)]
    pub sql: SqlConfig,
    #[serde(default)]
    pub migrations: Vec<Migration>,
    pub handlers: Vec<Handler>,
}

impl Deployment {
    pub fn validate(&self) -> Result<()> {
        self.sql.validate()?;
        ensure!(
            self.sql.enabled || self.migrations.is_empty(),
            "migrations require SQL to be enabled"
        );
        crate::persistence::sql::validate_migrations(&self.migrations, self.sql.migration_target)?;
        ensure!(
            crate::identifier::is_valid(&self.name),
            "invalid application name"
        );
        ensure!(!self.handlers.is_empty(), "application must have handlers");
        let mut names = std::collections::HashSet::new();
        for handler in &self.handlers {
            ensure!(
                crate::identifier::is_valid(&handler.name) && names.insert(&handler.name),
                "invalid or duplicate handler name"
            );
            ensure!(
                matches!(
                    handler.method.as_str(),
                    "GET" | "HEAD" | "POST" | "PUT" | "PATCH" | "DELETE" | "OPTIONS"
                ),
                "unsupported HTTP method"
            );
        }
        Ok(())
    }
}
