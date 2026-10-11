use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The alias definitions file under a dotfiles root (`<root>/cli/definitions.toml`).
pub fn definitions_path(root: &Path) -> PathBuf {
    root.join("cli").join("definitions.toml")
}

/// Resolve the dotfiles repo root, in this order:
/// 1. the `DOTFILES_DIR` environment variable at run time (when non-empty),
/// 2. the root baked in at build time (`cli/build.rs`), when it still exists,
/// 3. the nearest ancestor of the running exe that holds `cli/definitions.toml`.
pub fn dotfiles_root() -> Option<PathBuf> {
    resolve_root(
        std::env::var_os("DOTFILES_DIR"),
        Path::new(env!("DOTFILES_DIR")),
        std::env::current_exe,
    )
}

fn resolve_root(
    runtime: Option<OsString>,
    compiled: &Path,
    current_exe: impl FnOnce() -> std::io::Result<PathBuf>,
) -> Option<PathBuf> {
    if let Some(dir) = runtime.filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    if compiled.exists() {
        return Some(compiled.to_path_buf());
    }
    let exe = current_exe().ok()?;
    exe.ancestors()
        .find(|dir| definitions_path(dir).exists())
        .map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "dotcli-paths-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn no_exe() -> std::io::Result<PathBuf> {
        Err(std::io::Error::other("no exe"))
    }

    #[test]
    fn runtime_env_wins() {
        let got = resolve_root(
            Some("X:/from-env".into()),
            Path::new(env!("CARGO_MANIFEST_DIR")),
            no_exe,
        );
        assert_eq!(got, Some(PathBuf::from("X:/from-env")));
    }

    #[test]
    fn empty_runtime_env_is_ignored() {
        let compiled = Path::new(env!("CARGO_MANIFEST_DIR"));
        let got = resolve_root(Some(OsString::new()), compiled, no_exe);
        assert_eq!(got, Some(compiled.to_path_buf()));
    }

    #[test]
    fn compiled_root_used_when_present() {
        let compiled = Path::new(env!("DOTFILES_DIR"));
        let got = resolve_root(None, compiled, no_exe);
        assert_eq!(got, Some(compiled.to_path_buf()));
        assert!(definitions_path(compiled).exists());
    }

    #[test]
    fn exe_ancestor_holding_cli_definitions_is_root_not_cli_dir() {
        let root = temp_dir("exe");
        let release = root.join("cli").join("target").join("release");
        std::fs::create_dir_all(&release).unwrap();
        std::fs::write(definitions_path(&root), "").unwrap();
        let exe = release.join("dotcli.exe");
        let got = resolve_root(None, &root.join("missing"), || Ok(exe));
        assert_eq!(got, Some(root.clone()));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn nothing_found_is_none() {
        let root = temp_dir("none");
        let exe = root.join("bin").join("dotcli.exe");
        let got = resolve_root(None, &root.join("missing"), || Ok(exe));
        assert_eq!(got, None);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
