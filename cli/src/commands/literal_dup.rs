//! Scan for duplicate array/object *data literals* (single-source constant candidates).
//!
//! dup-scan は token 構造の clone を (内容を無視して) 検出し、string-dup は単一の文字列
//! リテラルを検出する。本コマンドは「内容まで同一の配列/オブジェクトリテラル」を検出する。
//! 例: `['hp','atk','def','spa','spd','spe']` が複数ファイルに散在する重複定数。
//!
//! token 化では token に内容 (text) が残らないため、ソースから直接リテラルを切り出して
//! 正規化 (空白除去 + クォート統一) し、内容ハッシュで出現回数を集計する。
//! 「データリテラル」= 文字列/数値/識別子/カンマ/コロン/ネスト[]{} のみで構成され、
//! `()`/`=>`/`=`/`;`/メンバ`.` を含まないもの (式やコードブロックを除外)。数値の小数点 (`0.5`) と
//! リテラル内のコメントは許容する。非 ASCII の文字列・識別子は元の文字のまま正規化キーに残る。

use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use super::dup_scan::{rust_quote_at, rust_raw_string_at, walk_files, RustQuote, DEFAULT_EXTS};

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

/// `#` を行コメントに使う言語の拡張子。これ以外は `//` と `/* */` をコメントとする。
const HASH_COMMENT_EXTS: &[&str] = &["py", "pyi", "pyw", "sh", "bash", "zsh", "ps1", "psm1", "psd1"];

/// 拡張子から決まる字句規則。
#[derive(Debug, Clone, Copy)]
struct Lang {
    /// .rs: `'` を char literal / lifetime として読み、raw string を文字列とする。
    rust: bool,
    /// true なら `#` が行コメント、false なら `//` と `/* */` がコメント。
    hash_comments: bool,
}

impl Lang {
    fn from_ext(ext: &str) -> Self {
        Lang {
            rust: ext.eq_ignore_ascii_case("rs"),
            hash_comments: HASH_COMMENT_EXTS.iter().any(|e| ext.eq_ignore_ascii_case(e)),
        }
    }
}

/// b[i] (行コメント開始) から改行 (or EOF) の位置を返す。
fn skip_line_comment(b: &[u8], i: usize) -> usize {
    b[i..].iter().position(|&ch| ch == b'\n').map_or(b.len(), |p| i + p)
}

/// b[i] は `/*` の `/`。閉じ `*/` の次のインデックスを返す。閉じがなければ None。
fn skip_block_comment(b: &[u8], i: usize) -> Option<usize> {
    b[i + 2..].windows(2).position(|w| w == b"*/").map(|p| i + 2 + p + 2)
}

/// b[i] == '.' が数値リテラルの小数点か (`0.5`, `1_000.25`, `1.5e3`)。
/// 直前が数字列で、その数字列が数字で始まり識別子や `.` に続いていないこと、直後が数字であることを要求する。
/// メンバアクセス `a.b`、tuple field `t1.0`、range `1..3`、`1.2.3` は false。
fn is_decimal_point(b: &[u8], i: usize) -> bool {
    if i == 0 || !b.get(i + 1).is_some_and(|ch| ch.is_ascii_digit()) {
        return false;
    }
    let mut j = i;
    while j > 0 && (b[j - 1].is_ascii_digit() || b[j - 1] == b'_') {
        j -= 1;
    }
    if j == i || !b[j].is_ascii_digit() {
        return false;
    }
    j == 0 || !(b[j - 1].is_ascii_alphanumeric() || b[j - 1] == b'_' || b[j - 1] == b'.' || b[j - 1] >= 0x80)
}

/// b[start] は '[' or '{'。データリテラルなら (close_idx, normalized, top_level_elements) を返す。
/// 式/コードブロック ( `()`/`=>`/`=`/`;`/メンバ`.` 等を含む) は None。リテラル内のコメントは読み飛ばす。
/// `lang.rust` では char literal を要素、raw string を文字列要素とし、lifetime を含むものは None。
fn scan_data_literal(b: &[u8], start: usize, lang: Lang) -> Option<(usize, String, usize)> {
    let n = b.len();
    let open = b[start];
    let close = if open == b'[' { b']' } else { b'}' };
    let mut depth: i32 = 0;
    let mut i = start;
    // 元バイト列をそのまま積む (UTF-8 の多バイト文字を 1 バイトずつ char 化しない)。
    let mut norm: Vec<u8> = Vec::new();
    let mut top_commas = 0usize;
    let mut nonempty = false;

    while i < n {
        let c = b[i];
        if lang.rust {
            if let Some(raw) = rust_raw_string_at(b, i) {
                norm.push(b'\'');
                norm.extend_from_slice(&b[raw.content_start..raw.content_end]);
                norm.push(b'\'');
                nonempty = true;
                i = raw.end;
                continue;
            }
            if c == b'\'' {
                let RustQuote::Char { end } = rust_quote_at(b, i) else { return None };
                norm.push(b'\'');
                norm.extend_from_slice(&b[i + 1..end - 1]);
                norm.push(b'\'');
                nonempty = true;
                i = end;
                continue;
            }
        }
        match c {
            b'[' | b'{' => { depth += 1; norm.push(c); i += 1; }
            b']' | b'}' => {
                depth -= 1;
                norm.push(c);
                i += 1;
                if depth == 0 {
                    if c != close { return None; }
                    let elements = if nonempty { top_commas + 1 } else { 0 };
                    // 区切りは ASCII のみで、文字列・要素は元の UTF-8 バイト列を丸ごと写すので valid。
                    return Some((i - 1, String::from_utf8(norm).ok()?, elements));
                }
                if depth < 0 { return None; }
            }
            b'\'' | b'"' | b'`' => {
                let e = skip_string(b, i);
                let inner_end = e.saturating_sub(1); // 閉じクォート位置 (or n)
                norm.push(b'\''); // クォートは ' に統一
                if i + 1 < inner_end {
                    norm.extend_from_slice(&b[i + 1..inner_end]);
                }
                norm.push(b'\'');
                nonempty = true;
                i = e;
            }
            b'#' if lang.hash_comments => { i = skip_line_comment(b, i); }
            b'/' if !lang.hash_comments && b.get(i + 1) == Some(&b'/') => { i = skip_line_comment(b, i); }
            b'/' if !lang.hash_comments && b.get(i + 1) == Some(&b'*') => { i = skip_block_comment(b, i)?; }
            b'.' if is_decimal_point(b, i) => { norm.push(b'.'); i += 1; }
            b',' => { if depth == 1 { top_commas += 1; } norm.push(b','); nonempty = true; i += 1; }
            b':' => { norm.push(b':'); nonempty = true; i += 1; }
            b'-' => { norm.push(b'-'); nonempty = true; i += 1; }
            b' ' | b'\t' | b'\r' | b'\n' => { i += 1; } // 空白除去
            // データ性 NG: 式・呼び出し・代入・メンバアクセス・テンプレート等
            b'(' | b')' | b';' | b'=' | b'.' | b'+' | b'*' | b'/' | b'<' | b'>'
            | b'!' | b'?' | b'&' | b'|' | b'@' | b'$' | b'%' | b'^' | b'~' | b'\\' => {
                return None;
            }
            _ => {
                // 識別子・数値・その他 (Unicode 識別子含む) は要素として許容
                norm.push(c);
                nonempty = true;
                i += 1;
            }
        }
    }
    None
}

/// 1 ソースから配列/オブジェクトのデータリテラルを抽出。戻り: (正規化, open文字, 要素数, 行)
/// コメント様式と Rust 固有の `'` / raw string の扱いは `ext` (拡張子) で決める。
fn extract_literals(src: &str, ext: &str) -> Vec<(String, char, usize, u32)> {
    let lang = Lang::from_ext(ext);
    let rust = lang.rust;
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
        if rust {
            if let Some(raw) = rust_raw_string_at(b, i) {
                i = raw.end;
                continue;
            }
            // 属性 `#[test]` / `#![allow(…)]` の括弧はデータではない (中身はそのまま読み進める)。
            if c == b'#' {
                let j = if b.get(i + 1) == Some(&b'!') { i + 2 } else { i + 1 };
                if b.get(j) == Some(&b'[') {
                    i = j + 1;
                    continue;
                }
            }
            if c == b'\'' {
                i = match rust_quote_at(b, i) {
                    RustQuote::Lifetime { end } | RustQuote::Char { end } => end,
                    RustQuote::Lone => i + 1,
                };
                continue;
            }
        }
        match c {
            // TODO: `//` と `/*` は # 系言語でもトップレベルではコメント扱いのまま (sh の `dir/*` が
            // `*/` まで飲み込む等)。ps1 の `` `" `` を skip_string が解さず崩れたクォート対応を
            // URL の `//` が行末で戻している例があり、PowerShell のエスケープ対応と同時に限定する。
            b'/' if b.get(i + 1) == Some(&b'/') => { i = skip_line_comment(b, i); }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i = skip_block_comment(b, i).unwrap_or(n);
            }
            // `#` は py/sh/ps1 系のみ行コメント。JS の `#private` や C の `#define` の行は読む。
            b'#' if lang.hash_comments => { i = skip_line_comment(b, i); }
            b'\'' | b'"' | b'`' => { i = skip_string(b, i); }
            b'[' | b'{' => {
                if let Some((end, norm, elements)) = scan_data_literal(b, i, lang) {
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
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
        for (norm, open, elements, line) in extract_literals(&src, ext) {
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
        // literal 昇順で tie-break (HashMap 順に依存せず、同値の並びと --top の境界を毎回同じにする)。
        sb.cmp(&sa).then_with(|| {
            let la = a.get("literal").and_then(|x| x.as_str()).unwrap_or("");
            let lb = b.get("literal").and_then(|x| x.as_str()).unwrap_or("");
            la.cmp(lb)
        })
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

#[cfg(test)]
mod rust_quote_tests {
    use super::*;

    const FIXTURE: &str = include_str!("testdata/rust_quotes.txt");

    #[test]
    fn rust_quote_literal_dup_finds_array_after_lifetimes() {
        let got = extract_literals(FIXTURE, "rs");
        assert!(
            got.contains(&("[1,2,3]".to_string(), '[', 3, 2)),
            "{got:?}"
        );
    }

    #[test]
    fn rust_quote_literal_dup_reads_char_and_raw_elements() {
        let got = extract_literals("v = [r#\"a\"#, 'b', '\\'', \"c\"]", "rs");
        assert_eq!(got, vec![("['a','b','\\'','c']".to_string(), '[', 4, 1)]);
        // A lifetime inside brackets is not data.
        assert!(extract_literals("x = [T, 'a, V]", "rs").is_empty());
    }

    #[test]
    fn rust_quote_rules_do_not_apply_to_ts() {
        // In TS `'` always opens a string: the quoted span hides the array.
        assert!(extract_literals("f<'a>(); h = [1, 2, 3]; g<'b>", "ts")
            .iter()
            .all(|l| l.0 != "[1,2,3]"));
        assert_eq!(
            extract_literals("h = ['a', 'b', 'c']", "ts"),
            vec![("['a','b','c']".to_string(), '[', 3, 1)]
        );
    }
}

#[cfg(test)]
mod literal_content_tests {
    use super::*;

    fn norms(src: &str, ext: &str) -> Vec<String> {
        extract_literals(src, ext).into_iter().map(|l| l.0).collect()
    }

    #[test]
    fn non_ascii_strings_keep_their_text_and_char_count() {
        let src = "export const X = ['ポケモン','あいう','えおか']; export const F = [0.1,0.2,0.3,0.4];";
        let got = extract_literals(src, "ts");
        assert_eq!(
            got,
            vec![
                ("['ポケモン','あいう','えおか']".to_string(), '[', 3, 1),
                ("[0.1,0.2,0.3,0.4]".to_string(), '[', 4, 1),
            ]
        );
        assert_eq!(got[0].0.chars().count(), 20);
        // Rust char literals and raw strings with multi-byte content.
        assert_eq!(
            norms("v = ['あ', 'い', r\"う\"]", "rs"),
            vec!["['あ','い','う']".to_string()]
        );
        // Non-ASCII identifiers stay intact as elements.
        assert_eq!(norms("v = [変数, 値, 名前]", "ts"), vec!["[変数,値,名前]".to_string()]);
    }

    #[test]
    fn decimal_point_only_inside_a_number() {
        assert_eq!(norms("v = [-0.5, 1.5e3, 1_000.25, 2]", "ts"), vec!["[-0.5,1.5e3,1_000.25,2]".to_string()]);
        // Member access, tuple fields, ranges and dotted versions are not data.
        assert!(norms("v = [a.b, c, d]", "ts").is_empty());
        assert!(norms("v = [t1.0, t1.1, t1.2]", "rs").is_empty());
        assert!(norms("v = [t.0, t.1, t.2]", "rs").is_empty());
        assert!(norms("v = [_1.0, b, c]", "ts").is_empty());
        assert!(norms("v = [1..3, 4, 5]", "rs").is_empty());
        assert!(norms("v = [1.2.3, 4, 5]", "ts").is_empty());
        assert!(norms("v = [1., 2, 3]", "ts").is_empty());
        assert!(norms("v = [.5, 2, 3]", "ts").is_empty());
    }

    #[test]
    fn c_style_comments_inside_a_literal_are_skipped() {
        let src = "const A = [\n  'a', // first\n  /* the second */ 'b',\n  'c', /* last */\n];\n";
        assert_eq!(extract_literals(src, "ts"), vec![("['a','b','c',]".to_string(), '[', 4, 1)]);
        // An unterminated block comment ends the scan without a literal.
        assert!(norms("const A = ['a', 'b', /* 'c']", "ts").is_empty());
    }

    #[test]
    fn hash_comments_only_in_hash_languages() {
        let py = "A = [\n    'a',  # first\n    'b',  # second.\n    'c',\n]\n";
        assert_eq!(extract_literals(py, "py"), vec![("['a','b','c',]".to_string(), '[', 4, 1)]);
        // `//` is floor division in Python, not a comment.
        assert!(norms("v = [n // 2, n // 3, n // 4]\n", "py").is_empty());
        // A line comment holding a literal hides it in shell.
        assert!(norms("# [1, 2, 3]\n", "sh").is_empty());
        // In JS `#` starts a private name, so the rest of the line is still read.
        assert_eq!(norms("class C { #cache = ['a', 'b', 'c']; }", "ts"), vec!["['a','b','c']".to_string()]);
        // C preprocessor lines are read too.
        assert_eq!(norms("#define XS {1, 2, 3}\n", "c"), vec!["{1,2,3}".to_string()]);
    }

    #[test]
    fn rust_attributes_are_not_data_but_their_line_is_read() {
        assert!(norms("#[test]\n#![allow(dead_code)]\n#[a, b, c]\n", "rs").is_empty());
        assert_eq!(norms("#[rustfmt::skip] const A: [u8; 3] = [1, 2, 3];", "rs"), vec!["[1,2,3]".to_string()]);
    }
}
