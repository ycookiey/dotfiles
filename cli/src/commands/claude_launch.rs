//! Argument handling shared by the claude launchers (claude-switch, claude-bedrock, claude-glm).

/// Flag added to every claude launch unless safe mode (`s`) or `rs` is given.
pub const SKIP_PERMISSIONS: &str = "--dangerously-skip-permissions";

/// Leading argument that launches claude without `SKIP_PERMISSIONS`.
pub const SAFE_ARG: &str = "s";

/// Build claude's arguments from a launcher's arguments:
/// - `r ...`  → `SKIP_PERMISSIONS /resume ...`
/// - `rs ...` → `/resume ...` (resume in safe mode)
/// - `s ...`  → `...` (safe mode)
/// - `...`    → `SKIP_PERMISSIONS ...`
pub fn claude_args(args: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    match args.first().map(String::as_str) {
        Some(mode @ ("r" | "rs")) => {
            if mode == "r" {
                out.push(SKIP_PERMISSIONS.into());
            }
            out.push("/resume".into());
            out.extend_from_slice(&args[1..]);
        }
        Some(SAFE_ARG) => out.extend_from_slice(&args[1..]),
        _ => {
            out.push(SKIP_PERMISSIONS.into());
            out.extend_from_slice(args);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn patterns() {
        let cases: &[(&[&str], &[&str])] = &[
            (&[], &[SKIP_PERMISSIONS]),
            (&["x", "y"], &[SKIP_PERMISSIONS, "x", "y"]),
            (&["s"], &[]),
            (&["s", "x"], &["x"]),
            (&["s", "r"], &["r"]),
            (&["r"], &[SKIP_PERMISSIONS, "/resume"]),
            (&["r", "a", "b"], &[SKIP_PERMISSIONS, "/resume", "a", "b"]),
            (&["rs"], &["/resume"]),
            (&["rs", "x"], &["/resume", "x"]),
        ];
        for (input, want) in cases {
            assert_eq!(claude_args(&v(input)), v(want), "input {input:?}");
        }
    }
}
