//! Format `string-dup` JSON output as a readable report (stdin).

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
    let extracted = as_u64(v.get("extracted_strings").unwrap_or(&Value::Null));
    let variants = as_u64(v.get("unique_variants").unwrap_or(&Value::Null));
    let min_chars = as_u64(v.get("min_chars").unwrap_or(&Value::Null));
    let min_words = as_u64(v.get("min_words").unwrap_or(&Value::Null));
    let min_occ = as_u64(v.get("min_occurrences").unwrap_or(&Value::Null));

    out.push_str(&format!(
        "String Dup Scan: {} files, {} strings ({} unique variants)\n",
        fmt_u64_commas(scanned_files),
        fmt_u64_commas(extracted),
        fmt_u64_commas(variants),
    ));
    out.push_str(&format!(
        "Filters: min_chars={}, min_words={}, min_occurrences={}\n",
        min_chars, min_words, min_occ,
    ));

    if let Some(arr) = v.get("language_breakdown").and_then(|x| x.as_array()) {
        if !arr.is_empty() {
            out.push_str("\nLanguage Breakdown\n");
            out.push_str(&format!(
                "  {:<10} {:<12} {:>6} {:>10}\n",
                "ext", "lang", "files", "strings"
            ));
            for o in arr {
                let ext = o.get("ext").and_then(|x| x.as_str()).unwrap_or("");
                let lang = o.get("lang").and_then(|x| x.as_str()).unwrap_or("");
                let f = as_u64(o.get("files").unwrap_or(&Value::Null));
                let s = as_u64(o.get("strings").unwrap_or(&Value::Null));
                out.push_str(&format!(
                    "  {:<10} {:<12} {:>6} {:>10}\n",
                    ext, lang, f, fmt_u64_commas(s)
                ));
            }
        }
    }

    if let Some(arr) = v.get("fallback_lexer_used").and_then(|x| x.as_array()) {
        if !arr.is_empty() {
            out.push_str("\n⚠ Fallback lexer used:\n");
            for o in arr {
                let ext = o.get("ext").and_then(|x| x.as_str()).unwrap_or("");
                let f = as_u64(o.get("files").unwrap_or(&Value::Null));
                out.push_str(&format!("  .{} ({} files)\n", ext, f));
            }
            out.push_str("  → results may be less accurate.\n");
        }
    }

    // Exact clusters
    let total_exact = as_u64(v.get("total_exact_clusters").unwrap_or(&Value::Null));
    let shown_exact = as_u64(v.get("shown_exact_clusters").unwrap_or(&Value::Null));
    out.push_str(&format!(
        "\nExact Clusters: {} total, showing {}\n",
        fmt_u64_commas(total_exact),
        fmt_u64_commas(shown_exact),
    ));

    if let Some(arr) = v.get("exact_clusters").and_then(|c| c.as_array()) {
        if arr.is_empty() {
            out.push_str("(no exact duplicates)\n");
        } else {
            for (i, c) in arr.iter().enumerate() {
                let chars = as_u64(c.get("char_count").unwrap_or(&Value::Null));
                let words = as_u64(c.get("word_count").unwrap_or(&Value::Null));
                let occ = as_u64(c.get("occurrence_count").unwrap_or(&Value::Null));
                let content = c.get("content").and_then(|x| x.as_str()).unwrap_or("");
                out.push_str(&format!(
                    "\n[#{}] {} chars, {} words, {} occurrences\n  \"{}\"\n",
                    i + 1, chars, words, occ, content
                ));
                if let Some(occs) = c.get("occurrences").and_then(|x| x.as_array()) {
                    for o in occs.iter().take(8) {
                        let f = o.get("file").and_then(|x| x.as_str()).unwrap_or("?");
                        let l = as_u64(o.get("line").unwrap_or(&Value::Null));
                        out.push_str(&format!("    - {}:{}\n", f, l));
                    }
                    if occs.len() > 8 {
                        out.push_str(&format!("    ... ({} more)\n", occs.len() - 8));
                    }
                }
            }
        }
    }

    // Similar clusters (only if --similarity jaccard)
    if let Some(sim) = v.get("similarity").and_then(|x| x.as_str()) {
        let threshold = v.get("threshold").and_then(|x| x.as_f64()).unwrap_or(0.0);
        let total = as_u64(v.get("total_similar_clusters").unwrap_or(&Value::Null));
        let shown = as_u64(v.get("shown_similar_clusters").unwrap_or(&Value::Null));
        out.push_str(&format!(
            "\nSimilar Clusters ({}, threshold={:.2}): {} total, showing {}\n",
            sim,
            threshold,
            fmt_u64_commas(total),
            fmt_u64_commas(shown),
        ));
        if let Some(arr) = v.get("similar_clusters").and_then(|c| c.as_array()) {
            if arr.is_empty() {
                out.push_str("(no similar clusters)\n");
            } else {
                for (i, c) in arr.iter().enumerate() {
                    let vc = as_u64(c.get("variant_count").unwrap_or(&Value::Null));
                    let total_occ = as_u64(c.get("total_occurrences").unwrap_or(&Value::Null));
                    out.push_str(&format!(
                        "\n[~#{}] {} variants, {} total occurrences\n",
                        i + 1, vc, total_occ
                    ));
                    if let Some(members) = c.get("members").and_then(|x| x.as_array()) {
                        for m in members.iter().take(6) {
                            let mocc = as_u64(m.get("occurrence_count").unwrap_or(&Value::Null));
                            let content = m.get("content").and_then(|x| x.as_str()).unwrap_or("");
                            out.push_str(&format!("  ({}×) \"{}\"\n", mocc, content));
                            if let Some(occs) = m.get("occurrences").and_then(|x| x.as_array()) {
                                for o in occs.iter().take(3) {
                                    let f = o.get("file").and_then(|x| x.as_str()).unwrap_or("?");
                                    let l = as_u64(o.get("line").unwrap_or(&Value::Null));
                                    out.push_str(&format!("      - {}:{}\n", f, l));
                                }
                                if occs.len() > 3 {
                                    out.push_str(&format!("      ... ({} more)\n", occs.len() - 3));
                                }
                            }
                        }
                        if members.len() > 6 {
                            out.push_str(&format!("  ... ({} more variants)\n", members.len() - 6));
                        }
                    }
                }
            }
        }
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
