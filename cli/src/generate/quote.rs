//! Quoting shared by the shell generators, so a value reaches the shell unchanged
//! whichever generator writes it.

/// bash single-quoted word: `'` becomes `'\''`.
pub fn bash_squote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Characters PowerShell takes as a single quote: `'` and U+2018 to U+201B.
fn is_ps_single_quote(c: char) -> bool {
    matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}')
}

/// Characters PowerShell takes as a double quote: `"` and U+201C to U+201E.
fn is_ps_double_quote(c: char) -> bool {
    matches!(c, '"' | '\u{201C}' | '\u{201D}' | '\u{201E}')
}

/// Body of a PowerShell single-quoted string: every single-quote character is doubled.
pub fn ps_squote_inner(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        out.push(c);
        if is_ps_single_quote(c) {
            out.push(c);
        }
    }
    out
}

/// Body of a PowerShell double-quoted string that expands nothing: backtick, `$` and
/// every double-quote character get a backtick in front.
pub fn ps_dquote_inner(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c == '`' || c == '$' || is_ps_double_quote(c) {
            out.push('`');
        }
        out.push(c);
    }
    out
}

/// PowerShell double-quoted path in which `{PROGRAM_FILES}` / `{USERPROFILE}` expand to
/// `$env:ProgramFiles` / `$env:USERPROFILE` and the rest stays literal.
pub fn ps_dquote_path_with_env(raw_path: &str) -> String {
    let mut out = String::from("\"");
    let mut rest = raw_path;
    loop {
        let next = [
            ("{PROGRAM_FILES}", "env:ProgramFiles"),
            ("{USERPROFILE}", "env:USERPROFILE"),
        ]
        .iter()
        .filter_map(|&(ph, var)| rest.find(ph).map(|i| (i, ph, var)))
        .min_by_key(|&(i, _, _)| i);
        let Some((i, ph, var)) = next else {
            out.push_str(&ps_dquote_inner(rest));
            out.push('"');
            return out;
        };
        out.push_str(&ps_dquote_inner(&rest[..i]));
        rest = &rest[i + ph.len()..];
        // `$env:ProgramFiles` followed by a name character would read a longer name.
        if rest
            .chars()
            .next()
            .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == ':')
        {
            out.push_str(&format!("${{{var}}}"));
        } else {
            out.push('$');
            out.push_str(var);
        }
    }
}

/// nushell string literal without escape processing: `'...'`, or a raw string
/// `r#'...'#` (with as many `#` as needed) when `s` itself contains `'`.
pub fn nu_literal(s: &str) -> String {
    if !s.contains('\'') {
        return format!("'{s}'");
    }
    let mut hashes = String::from("#");
    while s.contains(&format!("'{hashes}")) {
        hashes.push('#');
    }
    format!("r{hashes}'{s}'{hashes}")
}

/// nushell double-quoted string: `\` and `"` get a backslash in front.
pub fn nu_dquote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', r"\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    const SAMPLES: &[&str] = &[
        "plain",
        "it's",
        r#"say "hi""#,
        r"C:\p\x\",
        "$x `y $(z)",
        "\u{2018}curly\u{2019} \u{201C}dq\u{201D} \u{201A}\u{201B}\u{201E}",
        "",
    ];

    fn available(program: &str, args: &[&str]) -> bool {
        Command::new(program)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    #[test]
    fn ps_quoting() {
        assert_eq!(ps_squote_inner("it's \u{2019}"), "it''s \u{2019}\u{2019}");
        assert_eq!(ps_dquote_inner(r#"a"$`b"#), r#"a`"`$``b"#);
        assert_eq!(
            ps_dquote_path_with_env(r"{PROGRAM_FILES}\A B\a.exe"),
            r#""$env:ProgramFiles\A B\a.exe""#
        );
        assert_eq!(
            ps_dquote_path_with_env(r"{USERPROFILE}x\{PROGRAM_FILES}"),
            r#""${env:USERPROFILE}x\$env:ProgramFiles""#
        );
        assert_eq!(ps_dquote_path_with_env(r"C:\$a.exe"), r#""C:\`$a.exe""#);
    }

    #[test]
    fn nu_quoting() {
        assert_eq!(nu_dquote(r#"--p "x""#), r#""--p \"x\"""#);
        assert_eq!(nu_dquote(r"C:\a"), r#""C:\\a""#);
    }

    /// PowerShell reads each quoted form back as the original string.
    #[test]
    fn ps_quoting_round_trips_through_pwsh() {
        if !available(
            "pwsh",
            &["-NoProfile", "-NonInteractive", "-Command", "exit 0"],
        ) {
            return;
        }
        let mut script =
            String::from("[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)\n");
        for s in SAMPLES {
            script.push_str(&format!(
                "[Console]::Write('{}' + \"`n\")\n",
                ps_squote_inner(s)
            ));
            script.push_str(&format!(
                "[Console]::Write(\"{}\" + \"`n\")\n",
                ps_dquote_inner(s)
            ));
        }
        let path = std::env::temp_dir().join(format!(
            "dotcli-quote-{}.ps1",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        // BOM so PowerShell reads the curly quotes as UTF-8
        std::fs::write(&path, format!("\u{feff}{script}")).unwrap();
        let out = Command::new("pwsh")
            .args(["-NoProfile", "-NonInteractive", "-File"])
            .arg(&path)
            .output()
            .expect("failed to spawn pwsh");
        let _ = std::fs::remove_file(&path);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let expected: String = SAMPLES.iter().map(|s| format!("{s}\n{s}\n")).collect();
        assert_eq!(String::from_utf8_lossy(&out.stdout), expected);
    }

    #[test]
    fn nu_dquote_round_trips_through_nu() {
        if !available("nu", &["--version"]) {
            return;
        }
        for s in SAMPLES {
            let lit = nu_dquote(s);
            let out = Command::new("nu")
                .args(["-n", "-c", &format!("print -n {lit}")])
                .output()
                .expect("failed to spawn nu");
            assert!(out.status.success(), "nu failed on {lit}: {out:?}");
            assert_eq!(String::from_utf8_lossy(&out.stdout), *s, "literal {lit}");
        }
    }
}
