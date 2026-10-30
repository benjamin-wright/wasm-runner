use std::fs;

use super::server::Handler;
use anyhow::{Context, Result};

use super::manifest::ApplicationDefinition;

pub(crate) fn load_handlers(application: &ApplicationDefinition) -> Result<Vec<Handler>> {
    let mut functions = application.functions.iter().collect::<Vec<_>>();
    functions.sort_unstable_by_key(|(name, _)| *name);
    functions.into_iter().map(|(name, function)| {
        let component = fs::read(&function.module).with_context(|| format!(
            "loading component for function {name} at {}; run `wr build <DIRECTORY>` first if missing",
            function.module.display()
        ))?;
        Ok(Handler { name: name.clone(), method: function.http.method.to_string(), path: function.http.path.clone(), component })
    }).collect()
}
