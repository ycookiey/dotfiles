//! Scan for duplicate array/object *data literals* (single-source constant candidates).
//!
//! dup-scan は token 構造の clone を (内容を無視して) 検出し、string-dup は単一の文字列
//! リテラルを検出する。本コマンドは「内容まで同一の配列/オブジェクトリテラル」を検出する。
//! 例: `['hp','atk','def','spa','spd','spe']` が複数ファイルに散在する重複定数。
//!
//! 3 スキャナ共通の lexer (scan_common::Lexer) が返す token と文字列の位置から、括弧内の
//! ソースを読んでリテラルを組み立て、正規化 (空白除去 + クォート統一) し、内容ハッシュで
//! 出現回数を集計する。コメント・文字列の字句規則は拡張子の言語に従う。
//! 「データリテラル」= 文字列/数値/識別子/カンマ/コロン/ネスト[]{} のみで構成され、
//! `()`/`=>`/`=`/`;`/メンバ`.` を含まないもの (式やコードブロックを除外)。数値の小数点 (`0.5`) と
//! リテラル内のコメントは許容する。非 ASCII の文字列・識別子は元の文字のまま正規化キーに残る。

use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use super::dup_scan::DEFAULT_EXTS;
use super::scan_common::{display_path, walk_files, CodeKind, Event, Lexer, Syntax};

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

/// b[start] は '[' or '{' の token で、`lex` はその直後から読む。データリテラルなら
/// (normalized, top_level_elements) を返し、`lex` は閉じ括弧の直後まで進んでいる。
/// 式/コードブロック ( `()`/`=>`/`=`/`;`/メンバ`.` 等を含む) は None。コメントは lexer が読み飛ばす。
/// 文字列 (Rust の char literal・raw string を含む) は中身を `'…'` で囲んで 1 要素とし、
/// Rust の lifetime (`'` が code として残る) を含むものは None。
fn scan_data_literal(lex: &mut Lexer, b: &[u8], start: usize) -> Option<(String, usize)> {
    let open = b[start];
    let close = if open == b'[' { b']' } else { b'}' };
    let mut depth: i32 = 1;
    // 元バイト列をそのまま積む (UTF-8 の多バイト文字を 1 バイトずつ char 化しない)。
    let mut norm: Vec<u8> = vec![open];
    let mut top_commas = 0usize;
    let mut nonempty = false;

    for ev in lex {
        let (start, end) = match ev {
            Event::Str { content_start, content_end, .. } => {
                norm.push(b'\''); // クォートは ' に統一
                norm.extend_from_slice(&b[content_start..content_end]);
                norm.push(b'\'');
                nonempty = true;
                continue;
            }
            Event::Code { start, end, .. } => (start, end),
        };
        // token 間の空白は lexer が除いている。token 内はバイト単位で判定する。
        for (i, &c) in b.iter().enumerate().take(end).skip(start) {
            match c {
                b'[' | b'{' => { depth += 1; norm.push(c); }
                b']' | b'}' => {
                    depth -= 1;
                    norm.push(c);
                    if depth == 0 {
                        if c != close { return None; }
                        let elements = if nonempty { top_commas + 1 } else { 0 };
                        // 区切りは ASCII のみで、文字列・要素は元の UTF-8 バイト列を丸ごと写すので valid。
                        return Some((String::from_utf8(norm).ok()?, elements));
                    }
                }
                b'.' if is_decimal_point(b, i) => { norm.push(b'.'); }
                b',' => { if depth == 1 { top_commas += 1; } norm.push(b','); nonempty = true; }
                b':' | b'-' => { norm.push(c); nonempty = true; }
                // データ性 NG: 式・呼び出し・代入・メンバアクセス・テンプレート・Rust lifetime 等
                b'(' | b')' | b';' | b'=' | b'.' | b'+' | b'*' | b'/' | b'<' | b'>'
                | b'!' | b'?' | b'&' | b'|' | b'@' | b'$' | b'%' | b'^' | b'~' | b'\\'
                | b'\'' | b'"' | b'`' => {
                    return None;
                }
                _ => {
                    // 識別子・数値・その他 (Unicode 識別子含む) は要素として許容
                    norm.push(c);
                    nonempty = true;
                }
            }
        }
    }
    None
}

/// 1 ソースから配列/オブジェクトのデータリテラルを抽出。戻り: (正規化, open文字, 要素数, 行)
/// コメント・文字列の字句規則は `ext` (拡張子) の言語に従う (scan_common::Lexer)。
fn extract_literals(src: &str, ext: &str) -> Vec<(String, char, usize, u32)> {
    let syntax = Syntax::for_ext(ext);
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut lex = Lexer::new(src, syntax);
    // Rust 属性 `#[test]` / `#![allow(…)]` を開く `[` の位置。この括弧はデータではない (中身はそのまま読み進める)。
    let mut attr_bracket = None;

    while let Some(ev) = lex.next() {
        let Event::Code { kind: CodeKind::Punct(c), line, start, .. } = ev else { continue };
        match c {
            b'#' if syntax.rust => {
                let j = if b.get(start + 1) == Some(&b'!') { start + 2 } else { start + 1 };
                if b.get(j) == Some(&b'[') {
                    attr_bracket = Some(j);
                }
            }
            b'[' | b'{' if attr_bracket != Some(start) => {
                // 先読みはコピーで行い、データリテラルでなければ `[` の直後から読み直す (内側の入れ子を拾う)。
                let mut ahead = lex.clone();
                if let Some((norm, elements)) = scan_data_literal(&mut ahead, b, start) {
                    out.push((norm, c as char, elements, line));
                    lex = ahead;
                }
            }
            _ => {}
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
        let rel = display_path(p, root);
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
