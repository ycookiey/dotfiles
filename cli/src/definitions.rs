use serde::Deserialize;

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

impl Definitions {
    pub fn load() -> Self {
        let toml_str = std::fs::read_to_string("definitions.toml")
            .unwrap_or_else(|_| include_str!("../definitions.toml").to_string());
        toml::from_str(&toml_str).expect("Failed to parse definitions.toml")
    }
}
