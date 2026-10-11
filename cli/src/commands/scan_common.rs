//! Pieces shared by the source scanners (dup-scan, string-dup, literal-dup): the
//! extension -> language table, one lexer that splits source into code tokens and
//! string literals while skipping whitespace and comments, and the per-language
//! summary rows of their JSON output.
//!
//! Each scanner reads the same event stream: dup-scan turns it into token kinds,
//! string-dup keeps the string contents, and literal-dup rebuilds array/object
//! literals from the code bytes and strings between their brackets.

// TODO(cli-scan-17): move walk_files and display_path here from dup_scan / string_dup.

use serde_json::{Map, Value};
use std::collections::BTreeMap;

// ============================================================================
// Language table
// ============================================================================

/// Lexical family of a source file, picked from its extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lang {
    /// `//` and `/* */` comments, `"` `'` `` ` `` strings with `\` escapes.
    CFamily,
    /// `#` comments, triple-quoted strings.
    Python,
    /// `#` and `<# #>` comments, here-strings, `` ` `` escapes.
    PowerShell,
    /// `#` comments.
    Shell,
    /// `<!-- -->` comments, attribute values without escapes.
    Markup,
    /// Unknown extension: `//`, `#` and `/* */` comments.
    Fallback,
}

/// Extension (compared ignoring ASCII case) -> language. Anything else is Fallback.
const EXT_LANGS: &[(&str, Lang)] = &[
    ("rs", Lang::CFamily),
    ("ts", Lang::CFamily),
    ("tsx", Lang::CFamily),
    ("js", Lang::CFamily),
    ("jsx", Lang::CFamily),
    ("mjs", Lang::CFamily),
    ("cjs", Lang::CFamily),
    ("go", Lang::CFamily),
    ("java", Lang::CFamily),
    ("kt", Lang::CFamily),
    ("swift", Lang::CFamily),
    ("c", Lang::CFamily),
    ("h", Lang::CFamily),
    ("cpp", Lang::CFamily),
    ("hpp", Lang::CFamily),
    ("cc", Lang::CFamily),
    ("py", Lang::Python),
    ("pyi", Lang::Python),
    ("pyw", Lang::Python),
    ("ps1", Lang::PowerShell),
    ("psm1", Lang::PowerShell),
    ("psd1", Lang::PowerShell),
    ("sh", Lang::Shell),
    ("bash", Lang::Shell),
    ("zsh", Lang::Shell),
    ("html", Lang::Markup),
    ("vue", Lang::Markup),
    ("svelte", Lang::Markup),
    ("astro", Lang::Markup),
];

impl Lang {
    pub(crate) fn for_ext(ext: &str) -> Lang {
        EXT_LANGS
            .iter()
            .find(|(e, _)| e.eq_ignore_ascii_case(ext))
            .map_or(Lang::Fallback, |&(_, lang)| lang)
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Lang::CFamily => "c_family",
            Lang::Python => "python",
            Lang::PowerShell => "powershell",
            Lang::Shell => "shell",
            Lang::Markup => "markup",
            Lang::Fallback => "fallback",
        }
    }
}

/// How a file is lexed: its language, plus Rust's quote rules for `.rs` files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Syntax {
    pub lang: Lang,
    /// `.rs`: `'` opens a char literal or a lifetime / label, and raw strings exist.
    pub rust: bool,
}

impl Syntax {
    pub(crate) fn for_ext(ext: &str) -> Syntax {
        Syntax { lang: Lang::for_ext(ext), rust: ext.eq_ignore_ascii_case("rs") }
    }
}

// ============================================================================
// Events
// ============================================================================

/// A code token's kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum CodeKind {
    Id,
    Num,
    Punct(u8),
    /// Index into MULTI_PUNCT.
    MultiPunct(u8),
}

/// One lexed item. Byte offsets index the source; `line` is 1-based and is the
/// line the item starts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Event {
    /// A code token spanning `start..end`.
    Code { kind: CodeKind, line: u32, start: usize, end: usize },
    /// A string literal whose text between the quotes is `content_start..content_end`
    /// (escapes kept as written). `char_lit` marks a Rust char literal.
    Str { char_lit: bool, line: u32, content_start: usize, content_end: usize },
}

/// Multi-char punctuators (two- and three-char). A MultiPunct token stores its index here.
const MULTI_PUNCT: &[&[u8]] = &[
    b"==", b"!=", b"<=", b">=", b"&&", b"||", b"<<", b">>",
    b"->", b"=>", b"::", b"++", b"--", b"+=", b"-=", b"*=", b"/=", b"%=",
    b"&=", b"|=", b"^=", b"??", b"?.", b"..",
    b"...", b"..=", b"<<=", b">>=", b"===", b"!==", b"&&=", b"||=",
];

/// The MULTI_PUNCT index of `b`, if it is one. Only entries of `b`'s length can match.
pub(crate) fn multi_punct(b: &[u8]) -> Option<u8> {
    MULTI_PUNCT.iter().position(|p| *p == b).map(|i| i as u8)
}

// ============================================================================
// Rust quote forms
// ============================================================================
// In Rust a `'` may open a char literal or a lifetime / loop label, which has
// no closing quote, and raw strings take no backslash escapes. The lexer uses
// these only for `.rs` files; other languages keep treating `'` as a string quote.

/// What a `'` at `bytes[i]` starts in Rust source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RustQuote {
    /// Lifetime or loop label (`'a`, `'static`, `'_`, `'outer`); `end` is the index after the name.
    Lifetime { end: usize },
    /// Char or byte-char literal (`'x'`, `'\n'`, `'\''`, `'\u{1F600}'`, `'日'`);
    /// `end` is the index after the closing quote.
    Char { end: usize },
    /// A `'` that opens neither; callers step over the quote alone.
    Lone,
}

fn is_rust_ident_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c >= 0x80
}

fn utf8_len(first: u8) -> usize {
    match first {
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xFF => 4,
        _ => 1,
    }
}

/// Classifies the `'` at `bytes[i]`. A char literal closes right after one
/// character or one escape sequence on the same line; anything else starting
/// with an identifier character is a lifetime or label.
pub(crate) fn rust_quote_at(bytes: &[u8], i: usize) -> RustQuote {
    let n = bytes.len();
    let Some(&c) = bytes.get(i + 1) else {
        return RustQuote::Lone;
    };
    if c == b'\\' {
        // Escape: `'\n'`, `'\''`, `'\\'`, `'\x7f'`, `'\u{10FFFF}'` (closing quote at most at i + 11).
        // The byte right after the backslash is always part of the escape, even when it is `'`.
        let mut k = i + 3;
        while k < n && k <= i + 11 {
            match bytes[k] {
                b'\'' => return RustQuote::Char { end: k + 1 },
                b if b.is_ascii_whitespace() => break,
                _ => k += 1,
            }
        }
        return RustQuote::Lone;
    }
    if c != b'\'' && c != b'\n' && c != b'\r' {
        let close = i + 1 + utf8_len(c);
        if bytes.get(close) == Some(&b'\'') {
            return RustQuote::Char { end: close + 1 };
        }
    }
    if c.is_ascii_alphabetic() || c == b'_' || c >= 0x80 {
        let mut end = i + 1;
        while end < n && is_rust_ident_byte(bytes[end]) {
            end += 1;
        }
        return RustQuote::Lifetime { end };
    }
    RustQuote::Lone
}

/// Span of a Rust raw string literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RustRawStr {
    pub content_start: usize,
    /// Index of the closing `"`, or `bytes.len()` when unterminated.
    pub content_end: usize,
    /// Index after the closing `"` and its `#`s.
    pub end: usize,
}

/// Returns the raw string (`r"…"`, `r#"…"#`, `br##"…"##`, `cr"…"`) starting at
/// `bytes[i]`, or None. Requires a word boundary before `i`, so `r` inside an
/// identifier and raw identifiers such as `r#type` are not taken.
pub(crate) fn rust_raw_string_at(bytes: &[u8], i: usize) -> Option<RustRawStr> {
    let n = bytes.len();
    if i > 0 && is_rust_ident_byte(bytes[i - 1]) {
        return None;
    }
    let mut j = i;
    if matches!(bytes.get(j), Some(b'b') | Some(b'c')) {
        j += 1;
    }
    if bytes.get(j) != Some(&b'r') {
        return None;
    }
    j += 1;
    let hash_start = j;
    while j < n && bytes[j] == b'#' {
        j += 1;
    }
    let hashes = j - hash_start;
    if bytes.get(j) != Some(&b'"') {
        return None;
    }
    let content_start = j + 1;
    let mut k = content_start;
    while k < n {
        if bytes[k] == b'"'
            && k + 1 + hashes <= n
            && bytes[k + 1..k + 1 + hashes].iter().all(|&h| h == b'#')
        {
            return Some(RustRawStr { content_start, content_end: k, end: k + 1 + hashes });
        }
        k += 1;
    }
    Some(RustRawStr { content_start, content_end: n, end: n })
}

/// Newlines in `bytes[start..end]`.
fn count_newlines(bytes: &[u8], start: usize, end: usize) -> u32 {
    memchr::memchr_iter(b'\n', &bytes[start..end]).count() as u32
}

/// Index of the first `needle` in `hay` at or after `from`.
fn find_from(hay: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    memchr::memmem::find(&hay[from..], needle).map(|k| from + k)
}

// ============================================================================
// Lexer
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NumStyle {
    /// Digits, `.`, `_`, hex digits, `x`/`X`, and a sign after `e`/`E`.
    Generic,
    /// Digits, `.`, `_`, and the imaginary suffix `j`/`J`.
    Python,
    /// Digits, `.`, `_`.
    PowerShell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IdentStyle {
    /// Starts with a letter, `_` or `$`.
    Generic,
    /// Starts with a letter or `_`.
    Python,
    /// Starts with a letter, `_`, `$` or `@`, and continues over `::`.
    PowerShell,
}

/// The lexical rules of one Syntax, fixed for a whole file.
#[derive(Debug, Clone, Copy)]
struct Rules {
    line_comments: &'static [&'static [u8]],
    /// Opening and closing delimiters of the block comment.
    block_comment: Option<(&'static [u8], &'static [u8])>,
    /// Escape byte inside `"` / `'` strings; None when strings take no escapes.
    escape: Option<u8>,
    backtick_strings: bool,
    /// Python `"""…"""` / `'''…'''`.
    triple_quotes: bool,
    /// PowerShell `@"…"@` / `@'…'@`.
    here_strings: bool,
    rust: bool,
    num: NumStyle,
    ident: IdentStyle,
    /// Longest MULTI_PUNCT entry tried (1 = single-byte punctuators only).
    max_punct: usize,
}

impl Rules {
    fn of(syntax: Syntax) -> Rules {
        let c_like = Rules {
            line_comments: &[b"//"],
            block_comment: Some((b"/*", b"*/")),
            escape: Some(b'\\'),
            backtick_strings: true,
            triple_quotes: false,
            here_strings: false,
            rust: false,
            num: NumStyle::Generic,
            ident: IdentStyle::Generic,
            max_punct: 3,
        };
        match syntax.lang {
            Lang::CFamily => Rules { rust: syntax.rust, ..c_like },
            Lang::Shell => Rules { line_comments: &[b"#"], block_comment: None, ..c_like },
            Lang::Fallback => Rules { line_comments: &[b"//", b"#"], ..c_like },
            Lang::Python => Rules {
                line_comments: &[b"#"],
                block_comment: None,
                backtick_strings: false,
                triple_quotes: true,
                num: NumStyle::Python,
                ident: IdentStyle::Python,
                max_punct: 2,
                ..c_like
            },
            Lang::PowerShell => Rules {
                line_comments: &[b"#"],
                block_comment: Some((b"<#", b"#>")),
                escape: Some(b'`'),
                backtick_strings: false,
                here_strings: true,
                num: NumStyle::PowerShell,
                ident: IdentStyle::PowerShell,
                max_punct: 2,
                ..c_like
            },
            Lang::Markup => Rules {
                line_comments: &[],
                block_comment: Some((b"<!--", b"-->")),
                escape: None,
                backtick_strings: false,
                max_punct: 1,
                ..c_like
            },
        }
    }
}

/// Iterator over the code tokens and string literals of one source file. Whitespace
/// and comments yield nothing. Every other byte belongs to exactly one event, so the
/// events cover the file outside whitespace and comments. Cloning it saves the
/// position, so a caller can try reading ahead and drop the clone if that fails.
#[derive(Debug, Clone)]
pub(crate) struct Lexer<'a> {
    bytes: &'a [u8],
    rules: Rules,
    pos: usize,
    line: u32,
    /// The name of a Rust lifetime, returned right after its `'`.
    pending: Option<Event>,
}

impl<'a> Lexer<'a> {
    pub(crate) fn new(src: &'a str, syntax: Syntax) -> Self {
        Lexer { bytes: src.as_bytes(), rules: Rules::of(syntax), pos: 0, line: 1, pending: None }
    }

    fn code(&mut self, kind: CodeKind, start: usize, end: usize) -> Event {
        self.pos = end;
        Event::Code { kind, line: self.line, start, end }
    }

    /// A `"` / `'` / `` ` `` string opened at `start`; leaves `pos` after the closing quote.
    fn quoted(&mut self, start: usize) -> Event {
        let b = self.bytes;
        let n = b.len();
        let quote = b[start];
        let line = self.line;
        let mut k = start + 1;
        while k < n && b[k] != quote {
            if Some(b[k]) == self.rules.escape && k + 1 < n {
                if b[k + 1] == b'\n' {
                    self.line += 1;
                }
                k += 2;
                continue;
            }
            if b[k] == b'\n' {
                self.line += 1;
            }
            k += 1;
        }
        self.pos = if k < n { k + 1 } else { k };
        Event::Str { char_lit: false, line, content_start: start + 1, content_end: k }
    }

    /// A string from `content_start` to the next `close` (or the end of the file),
    /// taken without escapes; leaves `pos` after `close`.
    fn delimited(&mut self, content_start: usize, close: &[u8]) -> Event {
        let b = self.bytes;
        let line = self.line;
        let content_end = find_from(b, content_start, close).unwrap_or(b.len());
        let end = (content_end + close.len()).min(b.len());
        self.line += count_newlines(b, content_start, end);
        self.pos = end;
        Event::Str { char_lit: false, line, content_start, content_end }
    }
}

impl Iterator for Lexer<'_> {
    type Item = Event;

    fn next(&mut self) -> Option<Event> {
        if let Some(ev) = self.pending.take() {
            return Some(ev);
        }
        let b = self.bytes;
        let n = b.len();
        let r = self.rules;
        loop {
            let i = self.pos;
            let c = *b.get(i)?;

            if c == b'\n' {
                self.line += 1;
                self.pos += 1;
                continue;
            }
            if c.is_ascii_whitespace() {
                self.pos += 1;
                continue;
            }
            if let Some((open, close)) = r.block_comment {
                if b[i..].starts_with(open) {
                    let end = find_from(b, i + open.len(), close).map_or(n, |k| k + close.len());
                    self.line += count_newlines(b, i, end);
                    self.pos = end;
                    continue;
                }
            }
            if r.line_comments.iter().any(|p| b[i..].starts_with(p)) {
                self.pos = memchr::memchr(b'\n', &b[i..]).map_or(n, |k| i + k);
                continue;
            }

            // Rust: a lifetime / label is `'` + identifier, a char literal a string.
            if r.rust && c == b'\'' {
                return Some(match rust_quote_at(b, i) {
                    RustQuote::Lifetime { end } => {
                        let line = self.line;
                        self.pending = Some(Event::Code { kind: CodeKind::Id, line, start: i + 1, end });
                        self.pos = end;
                        Event::Code { kind: CodeKind::Punct(b'\''), line, start: i, end: i + 1 }
                    }
                    RustQuote::Char { end } => {
                        self.pos = end;
                        Event::Str { char_lit: true, line: self.line, content_start: i + 1, content_end: end - 1 }
                    }
                    RustQuote::Lone => self.code(CodeKind::Punct(b'\''), i, i + 1),
                });
            }
            // Rust raw string, checked before `r` / `br` / `cr` is read as an identifier.
            if r.rust && matches!(c, b'r' | b'b' | b'c') {
                if let Some(raw) = rust_raw_string_at(b, i) {
                    let line = self.line;
                    self.line += count_newlines(b, i, raw.end);
                    self.pos = raw.end;
                    return Some(Event::Str {
                        char_lit: false,
                        line,
                        content_start: raw.content_start,
                        content_end: raw.content_end,
                    });
                }
            }
            if r.here_strings && c == b'@' {
                if let Some(&q @ (b'"' | b'\'')) = b.get(i + 1) {
                    return Some(self.delimited(i + 2, &[q, b'@']));
                }
            }
            if r.triple_quotes
                && (c == b'"' || c == b'\'')
                && b.get(i + 1) == Some(&c)
                && b.get(i + 2) == Some(&c)
            {
                return Some(self.delimited(i + 3, &[c, c, c]));
            }
            if c == b'"' || c == b'\'' || (r.backtick_strings && c == b'`') {
                return Some(self.quoted(i));
            }

            if c.is_ascii_digit() {
                let mut k = i + 1;
                while let Some(&d) = b.get(k) {
                    let more = d.is_ascii_digit()
                        || d == b'.'
                        || d == b'_'
                        || match r.num {
                            NumStyle::Generic => {
                                d.is_ascii_hexdigit()
                                    || d == b'x'
                                    || d == b'X'
                                    || ((d == b'+' || d == b'-') && matches!(b[k - 1], b'e' | b'E'))
                            }
                            NumStyle::Python => d == b'j' || d == b'J',
                            NumStyle::PowerShell => false,
                        };
                    if !more {
                        break;
                    }
                    k += 1;
                }
                return Some(self.code(CodeKind::Num, i, k));
            }

            let ident_start = c.is_ascii_alphabetic()
                || c == b'_'
                || match r.ident {
                    IdentStyle::Generic => c == b'$',
                    IdentStyle::Python => false,
                    IdentStyle::PowerShell => c == b'$' || c == b'@',
                };
            if ident_start {
                let mut k = i + 1;
                while let Some(&d) = b.get(k) {
                    let more = d.is_ascii_alphanumeric()
                        || d == b'_'
                        || (r.ident == IdentStyle::PowerShell && d == b':' && b.get(k + 1) == Some(&b':'));
                    if !more {
                        break;
                    }
                    k += 1;
                }
                return Some(self.code(CodeKind::Id, i, k));
            }

            for len in (2..=r.max_punct).rev() {
                if i + len <= n {
                    if let Some(idx) = multi_punct(&b[i..i + len]) {
                        return Some(self.code(CodeKind::MultiPunct(idx), i, i + len));
                    }
                }
            }
            return Some(self.code(CodeKind::Punct(c), i, i + 1));
        }
    }
}

// ============================================================================
// Output rows
// ============================================================================

/// The `language_breakdown` rows: one per extension (`files` items of
/// (extension, language, count)), as {"ext", <lang_key>, "files", <count_key>},
/// ordered by count descending, then extension.
pub(crate) fn language_breakdown<'a>(
    files: impl IntoIterator<Item = (&'a str, Lang, u64)>,
    lang_key: &str,
    count_key: &str,
) -> Value {
    let mut by_ext: BTreeMap<&str, (Lang, u32, u64)> = BTreeMap::new();
    for (ext, lang, count) in files {
        let e = by_ext.entry(ext).or_insert((lang, 0, 0));
        e.0 = lang;
        e.1 += 1;
        e.2 += count;
    }
    let mut rows: Vec<(&str, (Lang, u32, u64))> = by_ext.into_iter().collect();
    // Stable, so equal counts keep the extension order.
    rows.sort_by(|a, b| b.1.2.cmp(&a.1.2));
    Value::Array(
        rows.into_iter()
            .map(|(ext, (lang, files, count))| {
                let mut row = Map::new();
                row.insert("ext".into(), ext.into());
                row.insert(lang_key.into(), lang.name().into());
                row.insert("files".into(), files.into());
                row.insert(count_key.into(), count.into());
                Value::Object(row)
            })
            .collect(),
    )
}

/// The `fallback_lexer_used` rows: {"ext", "files"} per extension lexed as Fallback.
pub(crate) fn fallback_warnings<'a>(files: impl IntoIterator<Item = (&'a str, Lang)>) -> Value {
    let mut by_ext: BTreeMap<&str, u32> = BTreeMap::new();
    for (ext, lang) in files {
        if lang == Lang::Fallback {
            *by_ext.entry(ext).or_default() += 1;
        }
    }
    Value::Array(
        by_ext
            .into_iter()
            .map(|(ext, count)| {
                let mut row = Map::new();
                row.insert("ext".into(), ext.into());
                row.insert("files".into(), count.into());
                Value::Object(row)
            })
            .collect(),
    )
}

#[cfg(test)]
mod rust_quote_tests {
    use super::*;

    #[test]
    fn rust_quote_classifies_char_literals() {
        let cases: &[(&[u8], usize)] = &[
            (b"'x'", 3),
            (b"' '", 3),
            (b"'\\n'", 4),
            (b"'\\''", 4),
            (b"'\\\\'", 4),
            (b"'\\x7f'", 6),
            (b"'\\u{1F600}'", 11),
            (b"'\\u{10FFFF}'", 12),
            ("'日'".as_bytes(), 5),
            ("'é'".as_bytes(), 4),
        ];
        for (src, end) in cases {
            assert_eq!(
                rust_quote_at(src, 0),
                RustQuote::Char { end: *end },
                "{}",
                String::from_utf8_lossy(src)
            );
        }
    }

    #[test]
    fn rust_quote_classifies_lifetimes_and_labels() {
        let cases: &[(&[u8], usize)] = &[
            (b"'a>", 2),
            (b"'a,'b>", 2),
            (b"'a", 2),
            (b"'static str", 7),
            (b"'_>", 2),
            (b"'outer: loop", 6),
            ("'é_x ".as_bytes(), 5),
        ];
        for (src, end) in cases {
            assert_eq!(
                rust_quote_at(src, 0),
                RustQuote::Lifetime { end: *end },
                "{}",
                String::from_utf8_lossy(src)
            );
        }
        assert_eq!(rust_quote_at(b"' ", 0), RustQuote::Lone);
        assert_eq!(rust_quote_at(b"''", 0), RustQuote::Lone);
        assert_eq!(rust_quote_at(b"'", 0), RustQuote::Lone);
    }

    #[test]
    fn rust_quote_raw_string_spans() {
        let raw = |content_start, content_end, end| RustRawStr { content_start, content_end, end };
        assert_eq!(rust_raw_string_at(b"r#\"a \"q\" b\"#;", 0), Some(raw(3, 10, 12)));
        assert_eq!(rust_raw_string_at(b"r\"C:\\\"", 0), Some(raw(2, 5, 6)));
        assert_eq!(rust_raw_string_at(b"br\"x\"", 0), Some(raw(3, 4, 5)));
        assert_eq!(rust_raw_string_at(b"cr##\"a\"#b\"##", 0), Some(raw(5, 9, 12)));
        assert_eq!(rust_raw_string_at(b"r#\"open", 0), Some(raw(3, 7, 7)));
        assert_eq!(rust_raw_string_at(b"r#type", 0), None);
        assert_eq!(rust_raw_string_at(b"b\"x\"", 0), None);
        assert_eq!(rust_raw_string_at(b"bar\"x\"", 2), None);
    }
}

#[cfg(test)]
mod lexer_tests {
    use super::*;

    /// The non-char string literals of `src` as (content, line).
    fn strings(src: &str, ext: &str) -> Vec<(String, u32)> {
        Lexer::new(src, Syntax::for_ext(ext))
            .filter_map(|ev| match ev {
                Event::Str { char_lit: false, line, content_start, content_end } => {
                    Some((src[content_start..content_end].to_string(), line))
                }
                _ => None,
            })
            .collect()
    }

    fn s(content: &str, line: u32) -> (String, u32) {
        (content.to_string(), line)
    }

    #[test]
    fn language_table_ignores_case_and_defaults_to_fallback() {
        assert_eq!(Syntax::for_ext("RS"), Syntax { lang: Lang::CFamily, rust: true });
        assert_eq!(Syntax::for_ext("ts"), Syntax { lang: Lang::CFamily, rust: false });
        assert_eq!(Lang::for_ext("pyi"), Lang::Python);
        assert_eq!(Lang::for_ext("zsh"), Lang::Shell);
        assert_eq!(Lang::for_ext("psd1"), Lang::PowerShell);
        assert_eq!(Lang::for_ext("Vue"), Lang::Markup);
        assert_eq!(Lang::for_ext("txt"), Lang::Fallback);
        assert_eq!(Lang::for_ext(""), Lang::Fallback);
    }

    #[test]
    fn python_triple_quotes_and_powershell_here_strings() {
        assert_eq!(
            strings("a = '''x 'y' \"z\"\nw'''\nb = \"c\"", "py"),
            vec![s("x 'y' \"z\"\nw", 1), s("c", 3)]
        );
        assert_eq!(
            strings("$a = @\"\nin \"q\" side\n\"@\n$b = 'c'", "ps1"),
            vec![s("\nin \"q\" side\n", 1), s("c", 4)]
        );
    }

    #[test]
    fn unterminated_strings_end_at_the_end_of_the_file() {
        // The content runs to the last byte, so a multi-byte tail is never cut.
        assert_eq!(strings("x = \"\"\"日本", "py"), vec![s("日本", 1)]);
        assert_eq!(strings("$x = @'\n日本", "ps1"), vec![s("\n日本", 1)]);
        assert_eq!(strings("x = \"日本", "ts"), vec![s("日本", 1)]);
        assert_eq!(strings("<!-- 'a' 日本", "html"), vec![]);
    }

    #[test]
    fn escapes_follow_the_language() {
        // PowerShell escapes with a backtick, not a backslash.
        assert_eq!(strings("$a = \"x`\"y\"; $b = 'p\\'", "ps1"), vec![s("x`\"y", 1), s("p\\", 1)]);
        assert_eq!(strings("a=\"x\\\"y\"", "sh"), vec![s("x\\\"y", 1)]);
        // Markup attribute values take no escapes.
        assert_eq!(strings("<a title=\"x\\\" b='c'>", "html"), vec![s("x\\", 1), s("c", 1)]);
        // Backticks are strings in C-family and shell, not in Python or PowerShell.
        assert_eq!(strings("a = `t`", "js"), vec![s("t", 1)]);
        assert_eq!(strings("a = `t`", "py"), vec![]);
    }

    #[test]
    fn comments_follow_the_language() {
        // `//` is floor division in Python and part of a path in shell.
        assert_eq!(strings("a = n // 2  # 'c'\nb = 'd'", "py"), vec![s("d", 2)]);
        assert_eq!(strings("ls dir/* 'x' # 'y'", "sh"), vec![s("x", 1)]);
        assert_eq!(strings("<# 'a'\n #> 'b' # 'c'", "ps1"), vec![s("b", 2)]);
        assert_eq!(strings("<!-- 'a' --> 'b' // 'c'", "html"), vec![s("b", 1), s("c", 1)]);
        assert_eq!(strings("# 'a'\n// 'b'\n/* 'c' */ 'd'", "txt"), vec![s("d", 3)]);
        // In JS `#` starts a private name.
        assert_eq!(strings("#x = 'a' // 'b'", "ts"), vec![s("a", 1)]);
    }

    #[test]
    fn events_are_ordered_and_disjoint() {
        let src = "fn f<'a>(x: &'a str) { let v = [1.5e-3, 0x1F]; r#\"q\"#; 'c' } // end\n$a::b @x";
        for ext in ["rs", "ts", "py", "ps1", "sh", "html", "txt"] {
            let mut prev_end = 0;
            for ev in Lexer::new(src, Syntax::for_ext(ext)) {
                let (start, end) = match ev {
                    Event::Code { start, end, .. } => (start, end),
                    // The content lies inside the literal; its quotes come before and after.
                    Event::Str { content_start, content_end, .. } => (content_start, content_end),
                };
                assert!(prev_end <= start && start <= end, "{ext}: {ev:?} after {prev_end}");
                prev_end = end;
            }
        }
    }
}
