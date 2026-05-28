//! Scan for duplicate array/object *data literals* (single-source constant candidates).
//!
//! dup-scan は token 構造の clone を (内容を無視して) 検出し、string-dup は単一の文字列
//! リテラルを検出する。本コマンドは「内容まで同一の配列/オブジェクトリテラル」を検出する。
//! 例: `['hp','atk','def','spa','spd','spe']` が複数ファイルに散在する重複定数。
//!
//! token 化では token に内容 (text) が残らないため、ソースから直接リテラルを切り出して
//! 正規化 (空白除去 + クォート統一) し、内容ハッシュで出現回数を集計する。
//! 「データリテラル」= 文字列/数値/識別子/カンマ/コロン/ネスト[]{} のみで構成され、
//! `()`/`=>`/`=`/`;`/メンバ`.` を含まないもの (式やコードブロックを除外)。

use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use super::dup_scan::{walk_files, DEFAULT_EXTS};

const DEFAULT_MIN_OCCURRENCES: usize = 3;
const DEFAULT_MIN_ELEMENTS: usize = 3;
const DEFAULT_MIN_CHARS: usize = 10;
const DEFAULT_TOP: usize = 30;

struct Args {
    path: PathBuf,
    exts: Vec<String>,
    min_occurrences: usize,
    min_elements: usize,
    min_chars: usize,
    top: usize,
    excludes: Vec<String>,
    no_gitignore: bool,
}

fn parse_args(args: &[String]) -> Args {
    let mut path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut exts: Vec<String> = DEFAULT_EXTS.iter().map(|s| s.to_string()).collect();
    let mut min_occurrences = DEFAULT_MIN_OCCURRENCES;
    let mut min_elements = DEFAULT_MIN_ELEMENTS;
    let mut min_chars = DEFAULT_MIN_CHARS;
    let mut top = DEFAULT_TOP;
    let mut excludes: Vec<String> = Vec::new();
    let mut no_gitignore = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--path" => { if i + 1 < args.len() { path = PathBuf::from(&args[i + 1]); i += 1; } }
            "--ext" => { if i + 1 < args.len() { exts = args[i + 1].split(',').map(|s| s.trim().to_string()).collect(); i += 1; } }
            "--min-occurrences" => { if i + 1 < args.len() { min_occurrences = args[i + 1].parse().unwrap_or(DEFAULT_MIN_OCCURRENCES); i += 1; } }
            "--min-elements" => { if i + 1 < args.len() { min_elements = args[i + 1].parse().unwrap_or(DEFAULT_MIN_ELEMENTS); i += 1; } }
            "--min-chars" => { if i + 1 < args.len() { min_chars = args[i + 1].parse().unwrap_or(DEFAULT_MIN_CHARS); i += 1; } }
            "--top" => { if i + 1 < args.len() { top = args[i + 1].parse().unwrap_or(DEFAULT_TOP); i += 1; } }
            "--exclude" => { if i + 1 < args.len() { excludes.push(args[i + 1].clone()); i += 1; } }
            "--no-gitignore" => { no_gitignore = true; }
            _ => {}
        }
        i += 1;
    }

    Args { path, exts, min_occurrences, min_elements, min_chars, top, excludes, no_gitignore }
}

/// 開きクォート b[start] から閉じ (or EOF) の次のインデックスを返す。
fn skip_string(b: &[u8], start: usize) -> usize {
    let q = b[start];
    let n = b.len();
    let mut i = start + 1;
    while i < n {
        if b[i] == b'\\' { i += 2; continue; }
        if b[i] == q { return i + 1; }
        i += 1;
    }
    n
}

/// b[start] は '[' or '{'。データリテラルなら (close_idx, normalized, top_level_elements) を返す。
/// 式/コードブロック ( `()`/`=>`/`=`/`;`/メンバ`.` 等を含む) は None。
fn scan_data_literal(b: &[u8], start: usize) -> Option<(usize, String, usize)> {
    let n = b.len();
    let open = b[start];
    let close = if open == b'[' { b']' } else { b'}' };
    let mut depth: i32 = 0;
    let mut i = start;
    let mut norm = String::new();
    let mut top_commas = 0usize;
    let mut nonempty = false;

    while i < n {
        let c = b[i];
        match c {
            b'[' | b'{' => { depth += 1; norm.push(c as char); i += 1; }
            b']' | b'}' => {
                depth -= 1;
                norm.push(c as char);
                i += 1;
                if depth == 0 {
                    if c != close { return None; }
                    let elements = if nonempty { top_commas + 1 } else { 0 };
                    return Some((i - 1, norm, elements));
                }
                if depth < 0 { return None; }
            }
            b'\'' | b'"' | b'`' => {
                let e = skip_string(b, i);
                let inner_end = e.saturating_sub(1); // 閉じクォート位置 (or n)
                norm.push('\''); // クォートは ' に統一
                if i + 1 < inner_end {
                    for &ch in &b[i + 1..inner_end] {
                        norm.push(ch as char);
                    }
                }
                norm.push('\'');
                nonempty = true;
                i = e;
            }
            b',' => { if depth == 1 { top_commas += 1; } norm.push(','); nonempty = true; i += 1; }
            b':' => { norm.push(':'); nonempty = true; i += 1; }
            b'-' => { norm.push('-'); nonempty = true; i += 1; }
            b' ' | b'\t' | b'\r' | b'\n' => { i += 1; } // 空白除去
            // データ性 NG: 式・呼び出し・代入・メンバアクセス・テンプレート等
            b'(' | b')' | b';' | b'=' | b'.' | b'+' | b'*' | b'/' | b'<' | b'>'
            | b'!' | b'?' | b'&' | b'|' | b'@' | b'$' | b'%' | b'^' | b'~' | b'\\' => {
                return None;
            }
            _ => {
                // 識別子・数値・その他 (Unicode 識別子含む) は要素として許容
                norm.push(c as char);
                nonempty = true;
                i += 1;
            }
        }
    }
    None
}

/// 1 ソースから配列/オブジェクトのデータリテラルを抽出。戻り: (正規化, open文字, 要素数, 行)
fn extract_literals(src: &str) -> Vec<(String, char, usize, u32)> {
    let b = src.as_bytes();
    let n = b.len();
    let mut out = Vec::new();

    // バイト位置 → 行番号
    let mut line_at = vec![1u32; n + 1];
    {
        let mut ln = 1u32;
        for (i, &ch) in b.iter().enumerate() {
            line_at[i] = ln;
            if ch == b'\n' { ln += 1; }
        }
        line_at[n] = ln;
    }

    let mut i = 0;
    while i < n {
        let c = b[i];
        match c {
            b'/' if i + 1 < n && b[i + 1] == b'/' => {
                i += 2;
                while i < n && b[i] != b'\n' { i += 1; }
            }
            b'/' if i + 1 < n && b[i + 1] == b'*' => {
                i += 2;
                while i + 1 < n && !(b[i] == b'*' && b[i + 1] == b'/') { i += 1; }
                i = (i + 2).min(n);
            }
            b'#' => {
                // shell/python 行コメント (簡易: ext で言語を限定しないため全 ext に適用)
                i += 1;
                while i < n && b[i] != b'\n' { i += 1; }
            }
            b'\'' | b'"' | b'`' => { i = skip_string(b, i); }
            b'[' | b'{' => {
                if let Some((end, norm, elements)) = scan_data_literal(b, i) {
                    out.push((norm, c as char, elements, line_at[i]));
                    i = end + 1;
                } else {
                    i += 1;
                }
            }
            _ => { i += 1; }
        }
    }
    out
}

pub fn run(args: &[String]) {
    let parsed = parse_args(args);
    let paths = walk_files(&parsed.path, &parsed.exts, &parsed.excludes, parsed.no_gitignore);
    let root = &parsed.path;

    // normalized -> occurrences
    let mut groups: HashMap<String, Vec<(String, u32, char, usize)>> = HashMap::new();
    for p in &paths {
        let src = match fs::read_to_string(p) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let rel = p
            .strip_prefix(root)
            .unwrap_or(p)
            .to_string_lossy()
            .replace('\\', "/");
        for (norm, open, elements, line) in extract_literals(&src) {
            if elements < parsed.min_elements {
                continue;
            }
            if norm.chars().count() < parsed.min_chars {
                continue;
            }
            groups
                .entry(norm)
                .or_default()
                .push((rel.clone(), line, open, elements));
        }
    }

    let mut clusters: Vec<Value> = groups
        .into_iter()
        .filter(|(_, occs)| occs.len() >= parsed.min_occurrences)
        .map(|(norm, occs)| {
            let open = occs[0].2;
            let elements = occs[0].3;
            let chars = norm.chars().count();
            json!({
                "literal": norm,
                "kind": if open == '[' { "array" } else { "object" },
                "elements": elements,
                "occurrence_count": occs.len(),
                "saved_chars": chars.saturating_mul(occs.len().saturating_sub(1)),
                "occurrences": occs.iter().map(|(f, l, _, _)| json!({"file": f, "line": l})).collect::<Vec<_>>(),
            })
        })
        .collect();

    // saved_chars 降順 (= 共通化で削減できる総文字量)。短い高頻度も長い低頻度も同じ土俵で評価。
    clusters.sort_by(|a, b| {
        let sa = a.get("saved_chars").and_then(|x| x.as_u64()).unwrap_or(0);
        let sb = b.get("saved_chars").and_then(|x| x.as_u64()).unwrap_or(0);
        sb.cmp(&sa)
    });
    let total = clusters.len();
    let shown: Vec<Value> = clusters.into_iter().take(parsed.top).collect();

    let result = json!({
        "scanned_files": paths.len(),
        "min_occurrences": parsed.min_occurrences,
        "min_elements": parsed.min_elements,
        "min_chars": parsed.min_chars,
        "total_clusters": total,
        "shown_clusters": shown.len(),
        "clusters": shown,
    });
    println!("{}", serde_json::to_string(&result).unwrap_or_default());
}
