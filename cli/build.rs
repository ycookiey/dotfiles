use serde::Deserialize;
use std::collections::HashSet;

#[derive(Deserialize)]
struct Definitions {
    #[serde(default)]
    alias: Vec<Named>,
    #[serde(default)]
    script: Vec<Named>,
    #[serde(default)]
    launcher: Vec<LauncherDef>,
    #[serde(default)]
    command: Vec<Named>,
}

#[derive(Deserialize)]
struct Named {
    name: String,
}

#[derive(Deserialize)]
struct LauncherDef {
    name: String,
    #[serde(rename = "type")]
    launcher_type: String,
    #[serde(default)]
    uri: Option<String>,
    #[serde(default)]
    app_name: Option<String>,
    #[serde(default)]
    path: Option<String>,
}

fn main() {
    println!("cargo:rerun-if-changed=definitions.toml");

    // Embed dotfiles root path at compile time
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let dotfiles_dir = std::path::Path::new(&manifest_dir).parent().unwrap();
    println!(
        "cargo:rustc-env=DOTFILES_DIR={}",
        dotfiles_dir.display()
    );

    let toml_str =
        std::fs::read_to_string("definitions.toml").expect("Failed to read definitions.toml");

    let defs: Definitions =
        toml::from_str(&toml_str).expect("definitions.toml: deserialization failed");

    // Duplicate name check
    let mut names = HashSet::new();
    for n in defs
        .alias
        .iter()
        .map(|a| &a.name)
        .chain(defs.script.iter().map(|s| &s.name))
        .chain(defs.launcher.iter().map(|l| &l.name))
        .chain(defs.command.iter().map(|c| &c.name))
    {
        assert!(names.insert(n.as_str()), "Duplicate name: {n}");
    }

    // Launcher required field check
    for l in &defs.launcher {
        match l.launcher_type.as_str() {
            "uri" => assert!(l.uri.is_some(), "{}: uri required for type=uri", l.name),
            "start_app" => assert!(
                l.app_name.is_some(),
                "{}: app_name required for type=start_app",
                l.name
            ),
            "exe" => assert!(l.path.is_some(), "{}: path required for type=exe", l.name),
            other => panic!("{}: unknown launcher type: {other}", l.name),
        }
    }
}
