mod bash;
mod nushell;
mod powershell;

use crate::definitions::Definitions;
use crate::paths::definitions_path;
use std::path::{Path, PathBuf};

/// Generate the alias files under `output_dir` (a dotfiles root) from
/// `<output_dir>/cli/definitions.toml`, falling back to the embedded copy when absent.
pub fn run(output_dir: &Path) -> anyhow::Result<()> {
    let output_dir = strip_unc(output_dir.canonicalize()?);
    let defs = Definitions::load(Some(&definitions_path(&output_dir)))?;

    let ps1_path = output_dir.join("generated-aliases.ps1");
    let nu_path = output_dir.join("nushell").join("generated-aliases.nu");
    let sh_path = output_dir.join("bash").join("generated-aliases.sh");

    let ps1 = powershell::generate(&defs, &output_dir);
    let nu = nushell::generate(&defs, &output_dir);
    let sh = bash::generate(&defs, &output_dir);

    std::fs::write(&ps1_path, &ps1)?;
    std::fs::write(&nu_path, &nu)?;
    std::fs::write(&sh_path, &sh)?;

    eprintln!("Generated: {}", ps1_path.display());
    eprintln!("Generated: {}", nu_path.display());
    eprintln!("Generated: {}", sh_path.display());
    Ok(())
}

/// Strip Windows `\\?\` UNC prefix from canonicalized paths
fn strip_unc(p: PathBuf) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(stripped) = s.strip_prefix(r"\\?\") {
        PathBuf::from(stripped)
    } else {
        p
    }
}
