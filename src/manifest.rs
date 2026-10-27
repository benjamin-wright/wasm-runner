use std::{
    collections::{HashMap, HashSet},
    fmt, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use wasm_runner_setup::Dependencies;

const APPLICATION_SCHEMA: &str = include_str!("../application.schema.json");

#[derive(Deserialize)]
pub(crate) struct ApplicationDefinition {
    pub name: String,
    #[serde(default)]
    pub dependencies: Dependencies,
    pub functions: HashMap<String, FunctionDefinition>,
}

#[derive(Deserialize)]
pub(crate) struct FunctionDefinition {
    #[serde(skip)]
    pub module: PathBuf,
    #[serde(default)]
    pub dependencies: Dependencies,
    pub http: RouteDefinition,
}

#[derive(Deserialize)]
pub(crate) struct RouteDefinition {
    pub method: HttpMethod,
    pub path: String,
}

#[derive(Clone, Copy, Deserialize, Hash, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub(crate) enum HttpMethod {
    Get,
    Head,
    Post,
    Put,
    Patch,
    Delete,
    Options,
}

impl fmt::Display for HttpMethod {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.pad(match self {
            Self::Get => "GET",
            Self::Head => "HEAD",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Options => "OPTIONS",
        })
    }
}

pub(crate) fn load(directory: &Path) -> Result<ApplicationDefinition> {
    let path = directory.join("application.yaml");
    let source = fs::read_to_string(&path)
        .with_context(|| format!("reading manifest {}", path.display()))?;
    let mut application =
        parse(&source).with_context(|| format!("parsing manifest {}", path.display()))?;
    let mut routes = HashSet::new();
    for (name, function) in &mut application.functions {
        function.module = directory.join(format!(".wr/components/{name}.wasm"));
        if !routes.insert((function.http.method, &function.http.path)) {
            bail!(
                "{}: /functions/{name}/http: duplicate method/path pair ({} {})",
                path.display(),
                function.http.method,
                function.http.path
            );
        }
    }
    for name in application.functions.keys() {
        let source = directory.join("functions").join(format!("{name}.rs"));
        let metadata = fs::metadata(&source).with_context(|| {
            format!(
                "{}: reading function source {}",
                path.display(),
                source.display()
            )
        })?;
        if !metadata.is_file() {
            bail!(
                "{}: function source {} is not a file",
                path.display(),
                source.display()
            );
        }
    }
    Ok(application)
}

fn parse(source: &str) -> Result<ApplicationDefinition> {
    let mut yaml: yaml_serde::Value = yaml_serde::from_str(source)?;
    yaml.apply_merge()?;
    let value = yaml_serde::from_value(yaml)?;
    validate_schema(&value)?;
    Ok(serde_json::from_value(value)?)
}

fn validate_schema(value: &serde_json::Value) -> Result<()> {
    let schema = serde_json::from_str(APPLICATION_SCHEMA)
        .expect("embedded application schema must be valid JSON");
    let validator =
        jsonschema::validator_for(&schema).expect("embedded application schema must be valid");

    let errors: Vec<_> = validator
        .iter_errors(value)
        .map(|error| {
            let path = error.instance_path().to_string();
            format!("{}: {error}", if path.is_empty() { "/" } else { &path })
        })
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        bail!("{}", errors.join("\n"))
    }
}
