//! Format `dup-scan` JSON output as a compact human-readable report (stdin).

use serde_json::Value;
use std::io::{self, Read};

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

fn as_u64(v: &Value) -> u64 {
    v.as_u64().or_else(|| v.as_i64().map(|i| i as u64)).unwrap_or(0)
}

pub fn format(v: &Value) -> String {
    let mut out = String::new();

    let scanned_files = as_u64(v.get("scanned_files").unwrap_or(&Value::Null));
    let scanned_tokens = as_u64(v.get("scanned_tokens").unwrap_or(&Value::Null));
    let min_tokens = as_u64(v.get("min_tokens").unwrap_or(&Value::Null));
    let min_lines = as_u64(v.get("min_lines").unwrap_or(&Value::Null));
    let total_clusters = as_u64(v.get("total_clusters").unwrap_or(&Value::Null));
    let shown_clusters = as_u64(v.get("shown_clusters").unwrap_or(&Value::Null));
    let max_gap = as_u64(v.get("max_gap").unwrap_or(&Value::Null));
    let type3_count = as_u64(v.get("type3_clusters").unwrap_or(&Value::Null));

    out.push_str(&format!(
        "Duplicate Scan: {} files, {} tokens (min_tokens={}, min_lines={}, max_gap={})\n",
        fmt_u64_commas(scanned_files),
        fmt_u64_commas(scanned_tokens),
        min_tokens,
        min_lines,
        max_gap,
    ));
    out.push_str(&format!(
        "Clusters: {} total ({} type-3), showing {}\n",
        fmt_u64_commas(total_clusters),
        fmt_u64_commas(type3_count),
        fmt_u64_commas(shown_clusters),
    ));

    if let Some(arr) = v.get("language_breakdown").and_then(|x| x.as_array()) {
        if !arr.is_empty() {
            out.push_str("\nLanguage Breakdown\n");
            out.push_str(&format!("  {:<10} {:<12} {:>6} {:>10}\n", "ext", "lexer", "files", "tokens"));
            for o in arr {
                let ext = o.get("ext").and_then(|x| x.as_str()).unwrap_or("");
                let lexer = o.get("lexer").and_then(|x| x.as_str()).unwrap_or("");
                let files = as_u64(o.get("files").unwrap_or(&Value::Null));
                let tokens = as_u64(o.get("tokens").unwrap_or(&Value::Null));
                out.push_str(&format!(
                    "  {:<10} {:<12} {:>6} {:>10}\n",
                    ext, lexer, files, fmt_u64_commas(tokens)
                ));
            }
        }
    }

    if let Some(arr) = v.get("fallback_lexer_used").and_then(|x| x.as_array()) {
        if !arr.is_empty() {
            out.push_str("\n⚠ Fallback lexer used (no dedicated tokenizer for these extensions):\n");
            for o in arr {
                let ext = o.get("ext").and_then(|x| x.as_str()).unwrap_or("");
                let files = as_u64(o.get("files").unwrap_or(&Value::Null));
                out.push_str(&format!("  .{} ({} files)\n", ext, files));
            }
            out.push_str("  → results may be less accurate. Add dedicated lexer if needed.\n");
        }
    }

    out.push('\n');

    let clusters = match v.get("clusters").and_then(|c| c.as_array()) {
        Some(a) => a,
        None => {
            out.push_str("(no clusters)\n");
            return out;
        }
    };

    if clusters.is_empty() {
        out.push_str("(no duplicates found)\n");
        return out;
    }

    for (idx, c) in clusters.iter().enumerate() {
        let tokens = as_u64(c.get("token_count").unwrap_or(&Value::Null));
        let match_tokens = as_u64(c.get("match_tokens").unwrap_or(&Value::Null));
        let gap_tokens = as_u64(c.get("gap_tokens").unwrap_or(&Value::Null));
        let lines = as_u64(c.get("line_count").unwrap_or(&Value::Null));
        let occ_count = as_u64(c.get("occurrence_count").unwrap_or(&Value::Null));
        let match_type = c
            .get("match_type")
            .and_then(|x| x.as_str())
            .unwrap_or("exact");

        let header = if gap_tokens > 0 {
            format!(
                "[#{}] ({}) {} match + {} gap = {} tokens, {} lines, {} occurrences\n",
                idx + 1,
                match_type,
                fmt_u64_commas(match_tokens),
                fmt_u64_commas(gap_tokens),
                fmt_u64_commas(tokens),
                fmt_u64_commas(lines),
                occ_count,
            )
        } else {
            format!(
                "[#{}] {} tokens, {} lines, {} occurrences\n",
                idx + 1,
                fmt_u64_commas(tokens),
                fmt_u64_commas(lines),
                occ_count,
            )
        };
        out.push_str(&header);

        if let Some(occs) = c.get("occurrences").and_then(|o| o.as_array()) {
            for o in occs {
                let file = o.get("file").and_then(|x| x.as_str()).unwrap_or("?");
                let s = as_u64(o.get("start_line").unwrap_or(&Value::Null));
                let e = as_u64(o.get("end_line").unwrap_or(&Value::Null));
                out.push_str(&format!("  - {}:{}-{}\n", file, s, e));
            }
        }

        if let Some(preview) = c.get("preview").and_then(|p| p.as_str()) {
            if !preview.is_empty() {
                out.push_str("  preview:\n");
                for line in preview.lines() {
                    out.push_str(&format!("    {}\n", line));
                }
            }
        }
        out.push('\n');
    }

    out
}

pub fn run() {
    let mut input = String::new();
    if io::stdin().read_to_string(&mut input).is_err() {
        eprintln!("error: failed to read stdin");
        std::process::exit(1);
    }
    let v: Value = match serde_json::from_str(&input) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: invalid JSON: {e}");
            std::process::exit(1);
        }
    };
    print!("{}", format(&v));
}
