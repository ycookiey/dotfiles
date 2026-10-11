//! Helpers shared by the stdin JSON report formatters (dup-scan, string-dup, literal-dup).

use serde_json::Value;
use std::io::{self, Read};

/// `1234567` → `"1,234,567"`.
pub fn fmt_u64_commas(n: u64) -> String {
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

/// JSON number as u64; negatives are cast, anything else is 0.
pub fn as_u64(v: &Value) -> u64 {
    v.as_u64().or_else(|| v.as_i64().map(|i| i as u64)).unwrap_or(0)
}

/// Read JSON from stdin and print `format`'s report; exit 1 on a read or parse error.
pub fn run_stdin_json(format: fn(&Value) -> String) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commas() {
        assert_eq!(fmt_u64_commas(0), "0");
        assert_eq!(fmt_u64_commas(999), "999");
        assert_eq!(fmt_u64_commas(1000), "1,000");
        assert_eq!(fmt_u64_commas(1234567), "1,234,567");
    }

    #[test]
    fn as_u64_values() {
        assert_eq!(as_u64(&serde_json::json!(5)), 5);
        assert_eq!(as_u64(&serde_json::json!(-1)), u64::MAX);
        assert_eq!(as_u64(&serde_json::json!("5")), 0);
        assert_eq!(as_u64(&Value::Null), 0);
    }
}
