use anyhow::Context;
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
pub struct Definitions {
    #[serde(default)]
    pub alias: Vec<Alias>,
    #[serde(default)]
    pub script: Vec<Script>,
    #[serde(default)]
    pub launcher: Vec<Launcher>,
    #[serde(default)]
    pub command: Vec<Command>,
}

#[derive(Deserialize)]
pub struct Alias {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Deserialize)]
pub struct Script {
    pub name: String,
    pub path: String,
}

#[derive(Deserialize)]
pub struct Launcher {
    pub name: String,
    #[serde(rename = "type")]
    pub launcher_type: LauncherType,
    #[serde(default = "default_true")]
    pub nushell: bool,
    #[serde(default)]
    pub uri: Option<String>,
    #[serde(default)]
    pub app_name: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub platform: Option<Platform>,
}

#[derive(Deserialize)]
pub struct Command {
    pub name: String,
    pub subcommand: String,
    pub modifies_shell: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Deserialize, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum LauncherType {
    Uri,
    StartApp,
    Exe,
}

#[derive(Deserialize, PartialEq, Clone)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Windows,
    Macos,
}

/// definitions.toml as it was when this binary was built.
const EMBEDDED: &str = include_str!("../definitions.toml");

impl Definitions {
    /// Load definitions from `path`. Falls back to the copy embedded at build time,
    /// with a warning on stderr, only when `path` does not exist; `None` uses the
    /// embedded copy directly.
    pub fn load(path: Option<&Path>) -> anyhow::Result<Self> {
        const EMBEDDED_NAME: &str = "embedded definitions.toml";
        let (toml_str, source) = match path {
            Some(p) => match std::fs::read_to_string(p) {
                Ok(s) => (s, p.display().to_string()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    eprintln!(
                        "warning: {} not found; using the definitions.toml embedded at build time",
                        p.display()
                    );
                    (EMBEDDED.to_string(), EMBEDDED_NAME.to_string())
                }
                Err(e) => {
                    return Err(anyhow::Error::new(e).context(format!("read {}", p.display())));
                }
            },
            None => (EMBEDDED.to_string(), EMBEDDED_NAME.to_string()),
        };
        toml::from_str(&toml_str).with_context(|| format!("parse {source}"))
    }
}
