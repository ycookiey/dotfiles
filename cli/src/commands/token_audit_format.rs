//! Format `token-audit static` / `token-audit session` JSON as bar charts (stdin).

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

fn fmt_u64_commas(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out.chars().rev().collect()
}

fn fmt_f64_pct(n: f64) -> String {
    format!("{:.1}%", n)
}

fn display_path(p: &str) -> String {
    if let Ok(home) = std::env::var("HOME") {
        if p.starts_with(&home) {
            return format!("~{}", p.trim_start_matches(&home));
        }
    }
    if let Ok(home) = std::env::var("USERPROFILE") {
        let home_norm = home.replace('\\', "/");
        let p_norm = p.replace('\\', "/");
        if p_norm.len() >= home_norm.len()
            && p_norm[..home_norm.len()].eq_ignore_ascii_case(&home_norm)
        {
            return format!("~{}", &p_norm[home_norm.len()..]);
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

fn format_static(v: &Value) -> String {
    let mut out = String::new();

    out.push_str("CLAUDE.md\n");
    if let Some(arr) = v.get("claude_md").and_then(|x| x.as_array()) {
        let max_tok = arr
            .iter()
            .map(|o| as_i64(o.get("est_tokens").unwrap_or(&Value::Null)))
            .max()
            .unwrap_or(0)
            .max(1) as f64;

        for o in arr {
            let path = o
                .get("path")
                .and_then(|x| x.as_str())
                .unwrap_or("?");
            let bytes = as_i64(o.get("bytes").unwrap_or(&Value::Null));
            let tok = as_i64(o.get("est_tokens").unwrap_or(&Value::Null));
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

    let skills_total = as_i64(
        v.get("skills_total_est_tokens")
            .unwrap_or(&Value::Null),
    );
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
        let max_tok = arr
            .iter()
            .map(|o| as_i64(o.get("est_tokens").unwrap_or(&Value::Null)))
            .max()
            .unwrap_or(0)
            .max(1) as f64;

        let mut items: Vec<_> = arr.iter().collect();
        items.sort_by(|a, b| {
            let ta = as_i64(a.get("est_tokens").unwrap_or(&Value::Null));
            let tb = as_i64(b.get("est_tokens").unwrap_or(&Value::Null));
            tb.cmp(&ta)
        });

        for o in items {
            let name = o
                .get("name")
                .and_then(|x| x.as_str())
                .unwrap_or("?");
            let tok = as_i64(o.get("est_tokens").unwrap_or(&Value::Null));
            let tok_f = tok as f64;
            out.push_str(&format!(
                "  {:<24} {:>6} tok  {}\n",
                name,
                fmt_u64_commas(tok as u64),
                bar(tok_f, max_tok, BAR_WIDTH)
            ));
        }
    }

    let plugin_count = as_i64(v.get("plugin_count").unwrap_or(&Value::Null));
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

    let ignore = v
        .get("claudeignore_exists")
        .and_then(|x| x.as_bool())
        .unwrap_or(false);
    out.push('\n');
    if ignore {
        out.push_str(".claudeignore  present\n");
    } else {
        out.push_str(".claudeignore  not found ⚠\n");
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

fn format_session(v: &Value) -> String {
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

    out.push_str("By Tool (cache_create)\n");
    if let Some(arr) = v.get("by_tool").and_then(|x| x.as_array()) {
        let max_cc = arr
            .iter()
            .map(|o| as_f64(o.get("cache_create").unwrap_or(&Value::Null)))
            .fold(0.0_f64, f64::max)
            .max(1.0);

        for o in arr {
            let tool = o
                .get("tool")
                .and_then(|x| x.as_str())
                .unwrap_or("?");
            let cc = as_f64(o.get("cache_create").unwrap_or(&Value::Null));
            let pct = as_f64(o.get("pct").unwrap_or(&Value::Null));
            let warn = if cc > THRESH_CACHE_CREATE { " ⚠" } else { "" };
            let cc_u = cc.round() as u64;
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

    out.push_str("\nTop Reads\n");
    if let Some(arr) = v.get("top_reads").and_then(|x| x.as_array()) {
        let max_tok = arr
            .iter()
            .map(|o| as_f64(o.get("total_cache_create").unwrap_or(&Value::Null)))
            .fold(0.0_f64, f64::max)
            .max(1.0);

        for o in arr {
            let file = o
                .get("file")
                .and_then(|x| x.as_str())
                .unwrap_or("?");
            let count = as_i64(o.get("count").unwrap_or(&Value::Null));
            let total = as_f64(o.get("total_cache_create").unwrap_or(&Value::Null));
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
            let max_cc = arr
                .iter()
                .map(|o| as_f64(o.get("cache_create").unwrap_or(&Value::Null)))
                .fold(0.0_f64, f64::max)
                .max(1.0);
            for o in arr.iter().take(20) {
                let tool = o
                    .get("tool")
                    .and_then(|x| x.as_str())
                    .unwrap_or("?");
                let file = o
                    .get("file")
                    .and_then(|x| x.as_str())
                    .unwrap_or("");
                let cc = as_f64(o.get("cache_create").unwrap_or(&Value::Null));
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

    if let Some(arr) = v.get("duplicate_reads").and_then(|x| x.as_array()) {
        if !arr.is_empty() {
            out.push_str("\nDuplicate reads\n");
            for o in arr.iter().take(20) {
                let file = o
                    .get("file")
                    .and_then(|x| x.as_str())
                    .unwrap_or("?");
                let count = as_i64(o.get("count").unwrap_or(&Value::Null));
                let total = as_f64(o.get("total_cache_create").unwrap_or(&Value::Null));
                let total_u = total.round() as u64;
                out.push_str(&format!(
                    "  {:<40} {:>3}×  {:>10} tok\n",
                    file,
                    count,
                    fmt_u64_commas(total_u)
                ));
            }
        }
    }

    if let Some(arr) = v.get("context_growth").and_then(|x| x.as_array()) {
        if !arr.is_empty() {
            out.push_str("\nContext growth (sampled)\n");
            let max_ctx = arr
                .iter()
                .map(|o| as_f64(o.get("total_ctx").unwrap_or(&Value::Null)))
                .fold(0.0_f64, f64::max)
                .max(1.0);
            for o in arr {
                let idx = as_i64(o.get("request_idx").unwrap_or(&Value::Null));
                let ctx = as_f64(o.get("total_ctx").unwrap_or(&Value::Null));
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
                let idx = as_i64(o.get("request_idx").unwrap_or(&Value::Null));
                let before = as_f64(o.get("before").unwrap_or(&Value::Null));
                let after = as_f64(o.get("after").unwrap_or(&Value::Null));
                let ratio = as_f64(o.get("ratio").unwrap_or(&Value::Null));
                let cc = as_f64(o.get("compaction_cache_create").unwrap_or(&Value::Null));
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
            let max_f = arr
                .iter()
                .map(|o| {
                    o.get("first_total_ctx")
                        .or_else(|| o.get("first_input_tokens"))
                        .map(as_f64)
                        .unwrap_or(0.0)
                })
                .fold(0.0_f64, f64::max)
                .max(1.0);
            for o in arr {
                let sid = o
                    .get("session")
                    .and_then(|x| x.as_str())
                    .unwrap_or("?");
                let ft = o
                    .get("first_total_ctx")
                    .or_else(|| o.get("first_input_tokens"))
                    .map(as_f64)
                    .unwrap_or(0.0);
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
            let max_m = arr
                .iter()
                .map(|o| as_f64(o.get("max_ctx").unwrap_or(&Value::Null)))
                .fold(0.0_f64, f64::max)
                .max(1.0);
            for o in arr {
                let sid = o
                    .get("session")
                    .and_then(|x| x.as_str())
                    .unwrap_or("?");
                let mx = as_f64(o.get("max_ctx").unwrap_or(&Value::Null));
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
