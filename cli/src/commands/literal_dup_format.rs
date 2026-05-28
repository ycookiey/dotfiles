//! Format `literal-dup` JSON output as a compact human-readable report (stdin).

use serde_json::Value;
use std::io::{self, Read};

fn as_u64(v: &Value) -> u64 {
    v.as_u64().or_else(|| v.as_i64().map(|i| i as u64)).unwrap_or(0)
}

pub fn format(v: &Value) -> String {
    let mut out = String::new();

    let files = as_u64(v.get("scanned_files").unwrap_or(&Value::Null));
    let min_occ = as_u64(v.get("min_occurrences").unwrap_or(&Value::Null));
    let min_el = as_u64(v.get("min_elements").unwrap_or(&Value::Null));
    let total = as_u64(v.get("total_clusters").unwrap_or(&Value::Null));
    let shown = as_u64(v.get("shown_clusters").unwrap_or(&Value::Null));

    out.push_str(&format!(
        "Literal Dup Scan: {} files (min_occurrences={}, min_elements={})\n",
        files, min_occ, min_el,
    ));
    out.push_str(&format!("Clusters: {} total, showing {}\n\n", total, shown));

    let clusters = match v.get("clusters").and_then(|c| c.as_array()) {
        Some(a) => a,
        None => {
            out.push_str("(no clusters)\n");
            return out;
        }
    };
    if clusters.is_empty() {
        out.push_str("(no duplicate literals found)\n");
        return out;
    }

    for (idx, c) in clusters.iter().enumerate() {
        let kind = c.get("kind").and_then(|x| x.as_str()).unwrap_or("");
        let elements = as_u64(c.get("elements").unwrap_or(&Value::Null));
        let occ = as_u64(c.get("occurrence_count").unwrap_or(&Value::Null));
        let saved = as_u64(c.get("saved_chars").unwrap_or(&Value::Null));
        let lit = c.get("literal").and_then(|x| x.as_str()).unwrap_or("");
        let preview: String = if lit.chars().count() > 80 {
            let s: String = lit.chars().take(77).collect();
            format!("{}...", s)
        } else {
            lit.to_string()
        };

        out.push_str(&format!(
            "[#{}] {} {} elems, {} occurrences, saved {} chars\n",
            idx + 1,
            kind,
            elements,
            occ,
            saved,
        ));
        out.push_str(&format!("  {}\n", preview));

        if let Some(occs) = c.get("occurrences").and_then(|o| o.as_array()) {
            for o in occs.iter().take(8) {
                let f = o.get("file").and_then(|x| x.as_str()).unwrap_or("?");
                let l = as_u64(o.get("line").unwrap_or(&Value::Null));
                out.push_str(&format!("  - {}:{}\n", f, l));
            }
            if occs.len() > 8 {
                out.push_str(&format!("  ... ({} more)\n", occs.len() - 8));
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
