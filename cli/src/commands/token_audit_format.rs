//! Format `token-audit static` / `token-audit session` JSON as bar charts (stdin).

use super::fmt_util::fmt_u64_commas;
use serde_json::Value;
use std::io::{self, Read};

const BAR_WIDTH: usize = 16;

const THRESH_CLAUDE_MD_TOK: i64 = 1500;
const THRESH_SKILLS_TOTAL: i64 = 800;
const THRESH_PLUGIN_COUNT: i64 = 3;
const THRESH_CACHE_CREATE: f64 = 5000.0;

fn bar(value: f64, max: f64, width: usize) -> String {
    if max <= 0.0 {
        return format!("[{}]", "░".repeat(width));
    }
    let filled = ((value / max) * width as f64).round() as usize;
    let filled = filled.min(width);
    let empty = width - filled;
    format!("[{}{}]", "█".repeat(filled), "░".repeat(empty))
}

fn fmt_f64_pct(n: f64) -> String {
    format!("{:.1}%", n)
}

fn display_path(p: &str) -> String {
    display_path_with(
        p,
        std::env::var("HOME").ok().as_deref(),
        std::env::var("USERPROFILE").ok().as_deref(),
    )
}

/// `p` with a leading home directory replaced by `~`. HOME is matched as
/// written (case-sensitive, either separator), USERPROFILE case-insensitively
/// after turning `\` into `/`. The home must end at a path separator in `p`,
/// so `C:/Users/ab` does not shorten `C:/Users/abc/x`.
fn display_path_with(p: &str, home: Option<&str>, userprofile: Option<&str>) -> String {
    let is_sep = |c: char| c == '/' || c == '\\';
    if let Some(home) = home.map(|h| h.trim_end_matches(is_sep)).filter(|h| !h.is_empty()) {
        if let Some(rest) = p.strip_prefix(home) {
            if rest.is_empty() || rest.starts_with(is_sep) {
                return format!("~{}", rest);
            }
        }
    }
    if let Some(home) = userprofile {
        let home_norm = home.replace('\\', "/");
        let home_norm = home_norm.trim_end_matches('/');
        let p_norm = p.replace('\\', "/");
        if !home_norm.is_empty() {
            if let (Some(head), Some(rest)) =
                (p_norm.get(..home_norm.len()), p_norm.get(home_norm.len()..))
            {
                if head.eq_ignore_ascii_case(home_norm) && (rest.is_empty() || rest.starts_with('/')) {
                    return format!("~{}", rest);
                }
            }
        }
    }
    p.to_string()
}

fn as_i64(v: &Value) -> i64 {
    v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)).unwrap_or(0)
}

fn as_f64(v: &Value) -> f64 {
    v.as_f64().or_else(|| v.as_i64().map(|i| i as f64)).unwrap_or(0.0)
}

/// `o[key]` read by `as_i64` (a missing key is 0).
fn int(o: &Value, key: &str) -> i64 {
    o.get(key).map_or(0, as_i64)
}

/// `o[key]` read by `as_f64` (a missing key is 0).
fn num(o: &Value, key: &str) -> f64 {
    o.get(key).map_or(0.0, as_f64)
}

/// Bar scale of an integer column: the largest `int(o, key)`, at least 1.
fn max_int(arr: &[Value], key: &str) -> f64 {
    arr.iter().map(|o| int(o, key)).max().unwrap_or(0).max(1) as f64
}

/// Bar scale of a numeric column: the largest `num(o, key)`, at least 1.
fn max_num(arr: &[Value], key: &str) -> f64 {
    arr.iter().map(|o| num(o, key)).fold(0.0_f64, f64::max).max(1.0)
}

/// A startup_costs item's first context size; older reports named it
/// first_input_tokens.
fn startup_ctx(o: &Value) -> f64 {
    o.get("first_total_ctx")
        .or_else(|| o.get("first_input_tokens"))
        .map_or(0.0, as_f64)
}

pub fn format_static(v: &Value) -> String {
    let mut out = String::new();

    out.push_str("CLAUDE.md\n");
    if let Some(arr) = v.get("claude_md").and_then(|x| x.as_array()) {
        let max_tok = max_int(arr, "est_tokens");

        for o in arr {
            let path = o
                .get("path")
                .and_then(|x| x.as_str())
                .unwrap_or("?");
            let bytes = int(o, "bytes");
            let tok = int(o, "est_tokens");
            let tok_f = tok as f64;
            let warn = if tok > THRESH_CLAUDE_MD_TOK { " ⚠" } else { "" };
            let dp = display_path(path);
            out.push_str(&format!(
                "  {:<26} {:>6} tok  {}  ({} bytes){}\n",
                dp,
                fmt_u64_commas(tok as u64),
                bar(tok_f, max_tok, BAR_WIDTH),
                bytes,
                warn
            ));
        }
    }

    let skills_total = int(v, "skills_total_est_tokens");
    let sk_warn = if skills_total > THRESH_SKILLS_TOTAL {
        " ⚠"
    } else {
        ""
    };
    out.push_str(&format!(
        "\nSkills  (total: {} tok{})\n",
        fmt_u64_commas(skills_total as u64),
        sk_warn
    ));

    if let Some(arr) = v.get("skills").and_then(|x| x.as_array()) {
        let max_tok = max_int(arr, "est_tokens");

        let mut items: Vec<_> = arr.iter().collect();
        items.sort_by(|a, b| {
            let ta = int(a, "est_tokens");
            let tb = int(b, "est_tokens");
            tb.cmp(&ta)
        });

        for o in items {
            let name = o
                .get("name")
                .and_then(|x| x.as_str())
                .unwrap_or("?");
            let tok = int(o, "est_tokens");
            let tok_f = tok as f64;
            out.push_str(&format!(
                "  {:<24} {:>6} tok  {}\n",
                name,
                fmt_u64_commas(tok as u64),
                bar(tok_f, max_tok, BAR_WIDTH)
            ));
        }
    }

    let plugin_count = int(v, "plugin_count");
    let pc_warn = if plugin_count > THRESH_PLUGIN_COUNT {
        " ⚠"
    } else {
        ""
    };
    out.push_str(&format!(
        "\nPlugins ({}{})\n",
        plugin_count,
        pc_warn
    ));

    if let Some(arr) = v.get("plugins").and_then(|x| x.as_array()) {
        for p in arr {
            if let Some(s) = p.as_str() {
                out.push_str(&format!("  {}\n", s));
            }
        }
    }

    if let Some(n) = v.get("mcp_server_count").and_then(|x| x.as_i64()) {
        out.push_str(&format!("\nMCP servers: {}\n", n));
    }

    if let Some(active) = v
        .get("token_audit_hook_active")
        .and_then(|x| x.as_bool())
    {
        out.push_str(&format!(
            "token_audit_hook: {}\n",
            if active { "active" } else { "inactive" }
        ));
    }

    out
}

pub fn format_session(v: &Value) -> String {
    let mut out = String::new();

    if let Some(n) = v.get("sessions_analyzed").and_then(|x| x.as_i64()) {
        out.push_str(&format!("Sessions analyzed: {}\n", n));
    }
    if let Some(n) = v.get("total_requests").and_then(|x| x.as_i64()) {
        out.push_str(&format!("Total requests: {}\n", n));
    }
    if out.ends_with('\n') {
        out.push('\n');
    }

    if let Some(arr) = v.get("by_tool").and_then(|x| x.as_array()) {
        let has_extra = arr.iter().any(|o| o.get("total_output").is_some());
        if has_extra {
            out.push_str("By Tool (Δctx=context growth, out=output, eff=out/Δctx)\n");
        } else {
            out.push_str("By Tool (Δctx)\n");
        }
        let max_cc = max_num(arr, "cache_create");

        for o in arr {
            let tool = o
                .get("tool")
                .and_then(|x| x.as_str())
                .unwrap_or("?");
            let cc = num(o, "cache_create");
            let pct = num(o, "pct");
            let warn = if cc > THRESH_CACHE_CREATE { " ⚠" } else { "" };
            let cc_u = cc.round() as u64;

            if has_extra {
                let total_output = num(o, "total_output");
                let efficiency = num(o, "efficiency");
                out.push_str(&format!(
                    "  {:<12} {:>10} Δ   {:>6}  out:{:>9}  eff:{:>4.1}  {}{}\n",
                    tool,
                    fmt_u64_commas(cc_u),
                    fmt_f64_pct(pct),
                    fmt_u64_commas(total_output.round() as u64),
                    efficiency,
                    bar(cc, max_cc, BAR_WIDTH),
                    warn
                ));
            } else {
                out.push_str(&format!(
                    "  {:<12} {:>10} tok  {:>6}  {}{}\n",
                    tool,
                    fmt_u64_commas(cc_u),
                    fmt_f64_pct(pct),
                    bar(cc, max_cc, BAR_WIDTH),
                    warn
                ));
            }
        }
    }

    if let Some(arr) = v.get("toolsearch_breakdown").and_then(|x| x.as_array()) {
        if !arr.is_empty() {
            out.push_str("\nToolSearch breakdown\n");
            let max_cc = max_num(arr, "cache_create");
            for o in arr.iter().take(15) {
                let query = o
                    .get("query")
                    .and_then(|x| x.as_str())
                    .unwrap_or("?");
                let cc = num(o, "cache_create");
                let count = int(o, "count");
                let cc_u = cc.round() as u64;
                out.push_str(&format!(
                    "  {:<50} {:>2}×  {:>10} tok  {}\n",
                    query,
                    count,
                    fmt_u64_commas(cc_u),
                    bar(cc, max_cc, BAR_WIDTH)
                ));
            }
        }
    }

    if let Some(arr) = v.get("top_reads").and_then(|x| x.as_array()) {
        if !arr.is_empty() {
            out.push_str("\nTop Reads\n");
        }
        let max_tok = max_num(arr, "total_cache_create");

        for o in arr {
            let file = o
                .get("file")
                .and_then(|x| x.as_str())
                .unwrap_or("?");
            let count = int(o, "count");
            let total = num(o, "total_cache_create");
            let warn = if total > THRESH_CACHE_CREATE { " ⚠" } else { "" };
            let total_u = total.round() as u64;
            out.push_str(&format!(
                "  {:<28} {:>2} reads  {:>10} tok  {}{}\n",
                file,
                count,
                fmt_u64_commas(total_u),
                bar(total, max_tok, BAR_WIDTH),
                warn
            ));
        }
    }

    if let Some(arr) = v.get("large_operations").and_then(|x| x.as_array()) {
        if !arr.is_empty() {
            out.push_str("\nLarge operations (cache_create > 5000)\n");
            let max_cc = max_num(arr, "cache_create");
            for o in arr.iter().take(20) {
                let tool = o
                    .get("tool")
                    .and_then(|x| x.as_str())
                    .unwrap_or("?");
                let file = o
                    .get("file")
                    .and_then(|x| x.as_str())
                    .unwrap_or("");
                let cc = num(o, "cache_create");
                let cc_u = cc.round() as u64;
                let tail = if file.is_empty() {
                    String::new()
                } else {
                    format!("  {}", file)
                };
                out.push_str(&format!(
                    "  {:<24} {:>10} tok  {}{}\n",
                    tool,
                    fmt_u64_commas(cc_u),
                    bar(cc, max_cc, BAR_WIDTH),
                    tail
                ));
            }
        }
    }

    if let Some(arr) = v.get("large_responses").and_then(|x| x.as_array()) {
        if !arr.is_empty() {
            out.push_str("\nLarge responses (output > 10000)\n");
            let max_output = max_num(arr, "output");
            for o in arr.iter().take(20) {
                let tool = o
                    .get("tool")
                    .and_then(|x| x.as_str())
                    .unwrap_or("?");
                let file = o
                    .get("file")
                    .and_then(|x| x.as_str())
                    .unwrap_or("");
                let output = num(o, "output");
                let cc = num(o, "cache_create");
                let efficiency = num(o, "efficiency");
                let output_u = output.round() as u64;
                let cc_u = cc.round() as u64;
                let idx = int(o, "request_idx");
                let eff_str = format!("{:>7.1}", efficiency);
                let tail = if file.is_empty() {
                    String::new()
                } else {
                    format!("  {}", file)
                };
                let line = format!(
                    "  req {:>6}  {:<24} out:{:>8} cc:{:>8} eff:{}  {}{}\n",
                    idx,
                    tool,
                    fmt_u64_commas(output_u),
                    fmt_u64_commas(cc_u),
                    eff_str,
                    bar(output, max_output, BAR_WIDTH),
                    tail
                );
                out.push_str(&line);
            }
        }
    }

    if let Some(arr) = v.get("duplicate_reads").and_then(|x| x.as_array()) {
        if !arr.is_empty() {
            out.push_str("\nDuplicate reads\n");
            let max_total = max_num(arr, "total_cache_create");
            for o in arr.iter().take(20) {
                let file = o
                    .get("file")
                    .and_then(|x| x.as_str())
                    .unwrap_or("?");
                let count = int(o, "count");
                let total = num(o, "total_cache_create");
                let total_u = total.round() as u64;
                out.push_str(&format!(
                    "  {:<40} {:>3}×  {:>10} tok  {}\n",
                    file,
                    count,
                    fmt_u64_commas(total_u),
                    bar(total, max_total, BAR_WIDTH)
                ));
            }
        }
    }

    if let Some(arr) = v.get("context_growth").and_then(|x| x.as_array()) {
        if !arr.is_empty() {
            out.push_str("\nContext growth (sampled)\n");
            let max_ctx = max_num(arr, "total_ctx");
            for o in arr {
                let idx = int(o, "request_idx");
                let ctx = num(o, "total_ctx");
                let ctx_u = ctx.round() as u64;
                out.push_str(&format!(
                    "  req {:>6}  {:>10} tok  {}\n",
                    idx,
                    fmt_u64_commas(ctx_u),
                    bar(ctx, max_ctx, BAR_WIDTH)
                ));
            }
        }
    }

    if let Some(arr) = v.get("compactions").and_then(|x| x.as_array()) {
        if !arr.is_empty() {
            out.push_str("\nCompactions\n");
            for o in arr {
                let idx = int(o, "request_idx");
                let before = num(o, "before");
                let after = num(o, "after");
                let ratio = num(o, "ratio");
                let cc = num(o, "compaction_cache_create");
                out.push_str(&format!(
                    "  req {:>6}  {} → {}  (ratio {:.2})  cache_create {}\n",
                    idx,
                    fmt_u64_commas(before.round() as u64),
                    fmt_u64_commas(after.round() as u64),
                    ratio,
                    fmt_u64_commas(cc.round() as u64)
                ));
            }
        }
    }

    if let Some(arr) = v.get("startup_costs").and_then(|x| x.as_array()) {
        if !arr.is_empty() {
            out.push_str("\nStartup costs (first_total_ctx)\n");
            let max_f = arr.iter().map(startup_ctx).fold(0.0_f64, f64::max).max(1.0);
            for o in arr {
                let sid = o
                    .get("session")
                    .and_then(|x| x.as_str())
                    .unwrap_or("?");
                let ft = startup_ctx(o);
                let ft_u = ft.round() as u64;
                out.push_str(&format!(
                    "  {:<40} {:>10} tok  {}\n",
                    sid,
                    fmt_u64_commas(ft_u),
                    bar(ft, max_f, BAR_WIDTH)
                ));
            }
        }
    }

    if let Some(arr) = v.get("per_session_max_ctx").and_then(|x| x.as_array()) {
        if !arr.is_empty() {
            out.push_str("\nPer-session max context\n");
            let max_m = max_num(arr, "max_ctx");
            for o in arr {
                let sid = o
                    .get("session")
                    .and_then(|x| x.as_str())
                    .unwrap_or("?");
                let mx = num(o, "max_ctx");
                let mx_u = mx.round() as u64;
                out.push_str(&format!(
                    "  {:<40} {:>10} tok  {}\n",
                    sid,
                    fmt_u64_commas(mx_u),
                    bar(mx, max_m, BAR_WIDTH)
                ));
            }
        }
    }

    out
}

pub fn run() {
    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf).unwrap_or_default();
    let v: Value = match serde_json::from_str(&buf) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("invalid JSON: {e}");
            std::process::exit(1);
        }
    };

    if v.get("error").is_some() {
        println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
        return;
    }

    if v.get("claude_md").is_some() {
        print!("{}", format_static(&v));
        return;
    }

    if v.get("session1").is_some() && v.get("session2").is_some() {
        println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
        return;
    }

    if v.get("by_tool").is_some() || v.get("sessions_analyzed").is_some() {
        print!("{}", format_session(&v));
        return;
    }

    eprintln!("unrecognized token-audit JSON shape");
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::display_path_with;

    #[test]
    fn userprofile_prefix_becomes_tilde() {
        let up = Some("C:\\Users\\abc");
        assert_eq!(display_path_with("C:/Users/abc/.claude/CLAUDE.md", None, up), "~/.claude/CLAUDE.md");
        assert_eq!(display_path_with("c:\\users\\ABC\\x", None, up), "~/x");
        assert_eq!(display_path_with("C:/Users/abc", None, up), "~");
        assert_eq!(display_path_with("C:/Users/abc/x", None, Some("C:\\Users\\abc\\")), "~/x");
        assert_eq!(display_path_with("D:/other/x", None, up), "D:/other/x");
    }

    #[test]
    fn home_must_end_at_a_separator() {
        assert_eq!(display_path_with("C:/Users/abc2/x", None, Some("C:\\Users\\abc")), "C:/Users/abc2/x");
        assert_eq!(display_path_with("/home/abc2/x", Some("/home/abc"), None), "/home/abc2/x");
    }

    #[test]
    fn home_prefix_is_removed_once() {
        assert_eq!(display_path_with("/h/h/x", Some("/h"), None), "~/h/x");
        assert_eq!(display_path_with("/home/abc", Some("/home/abc"), None), "~");
        assert_eq!(display_path_with("C:\\Users\\abc\\x", Some("C:\\Users\\abc"), None), "~\\x");
        // An empty HOME matches nothing.
        assert_eq!(display_path_with("/x", Some(""), None), "/x");
    }

    #[test]
    fn home_length_inside_a_multibyte_char_does_not_panic() {
        // "C:/Users/xあ": byte 12 (= len of the home) falls inside あ.
        let p = "C:/Users/xあいう/f";
        assert_eq!(display_path_with(p, Some("C:/Users/abc"), Some("C:\\Users\\abc")), p);
        // Byte 8 is the first byte of あ, a boundary, but the rest is no subpath.
        assert_eq!(display_path_with("C:/Usersあ/f", None, Some("C:/Users")), "C:/Usersあ/f");
    }
}
