//! Code duplication scanner. Token-normalized n-gram detection for refactor candidates.
//!
//! Walks source files under cwd (respecting .gitignore), normalizes
//! identifiers/literals/comments, and reports duplicate token sequences.

use rayon::prelude::*;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

const DEFAULT_MIN_TOKENS: usize = 30;
const DEFAULT_MIN_LINES: usize = 3;
const DEFAULT_TOP: usize = 50;

const DEFAULT_EXTS: &[&str] = &[
    "rs", "ts", "tsx", "js", "jsx", "mjs", "cjs",
    "py", "ps1", "psm1", "sh", "bash",
    "go", "java", "kt", "swift",
    "c", "h", "cpp", "hpp", "cc",
];

#[derive(Debug, Clone, Copy)]
struct Token {
    kind: TokenKind,
    line: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum TokenKind {
    Id,
    Num,
    Str,
    Punct(u8),
    PunctHash(u32),
}

struct FileTokens {
    path: PathBuf,
    tokens: Vec<Token>,
    src: String,
    ext: String,
    lexer: &'static str,
}

#[derive(Debug, Clone)]
struct Occurrence {
    file_idx: usize,
    start_line: u32,
    end_line: u32,
}

#[derive(Debug)]
struct Cluster {
    token_count: usize,  // total length (match + gap)
    match_tokens: usize, // exact-match tokens
    gap_tokens: usize,   // substitution-gap tokens
    line_count: usize,
    occurrences: Vec<Occurrence>,
    preview: String,
}

struct Args {
    path: PathBuf,
    exts: Vec<String>,
    min_tokens: usize,
    min_lines: usize,
    top: usize,
    gap: usize,
    excludes: Vec<String>,
    no_gitignore: bool,
}

fn parse_args(args: &[String]) -> Args {
    let mut path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut exts: Vec<String> = DEFAULT_EXTS.iter().map(|s| s.to_string()).collect();
    let mut min_tokens = DEFAULT_MIN_TOKENS;
    let mut min_lines = DEFAULT_MIN_LINES;
    let mut top = DEFAULT_TOP;
    let mut gap: usize = 0;
    let mut excludes: Vec<String> = Vec::new();
    let mut no_gitignore = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--path" => {
                if i + 1 < args.len() {
                    path = PathBuf::from(&args[i + 1]);
                    i += 1;
                }
            }
            "--ext" => {
                if i + 1 < args.len() {
                    exts = args[i + 1].split(',').map(|s| s.trim().to_string()).collect();
                    i += 1;
                }
            }
            "--min-tokens" => {
                if i + 1 < args.len() {
                    min_tokens = args[i + 1].parse().unwrap_or(DEFAULT_MIN_TOKENS);
                    i += 1;
                }
            }
            "--min-lines" => {
                if i + 1 < args.len() {
                    min_lines = args[i + 1].parse().unwrap_or(DEFAULT_MIN_LINES);
                    i += 1;
                }
            }
            "--top" => {
                if i + 1 < args.len() {
                    top = args[i + 1].parse().unwrap_or(DEFAULT_TOP);
                    i += 1;
                }
            }
            "--gap" => {
                if i + 1 < args.len() {
                    gap = args[i + 1].parse().unwrap_or(0);
                    i += 1;
                }
            }
            "--exclude" => {
                if i + 1 < args.len() {
                    excludes.push(args[i + 1].clone());
                    i += 1;
                }
            }
            "--no-gitignore" => {
                no_gitignore = true;
            }
            _ => {}
        }
        i += 1;
    }

    Args { path, exts, min_tokens, min_lines, top, gap, excludes, no_gitignore }
}

// ============================================================================
// File discovery (respects .gitignore via `ignore` crate)
// ============================================================================

fn walk_files(
    root: &Path,
    exts: &[String],
    excludes: &[String],
    no_gitignore: bool,
) -> Vec<PathBuf> {
    let ext_set: HashSet<&str> = exts.iter().map(|s| s.as_str()).collect();
    let mut builder = ignore::WalkBuilder::new(root);
    builder
        .git_ignore(!no_gitignore)
        .git_exclude(!no_gitignore)
        .git_global(!no_gitignore)
        .ignore(!no_gitignore)
        .hidden(true)
        .parents(!no_gitignore);

    let mut out = Vec::new();
    for entry in builder.build().flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !ext_set.contains(ext) {
            continue;
        }
        let path_str = path.to_string_lossy().replace('\\', "/");
        if excludes.iter().any(|e| path_str.contains(e.as_str())) {
            continue;
        }
        out.push(path.to_path_buf());
    }
    out
}

// ============================================================================
// Language dispatch
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lexer {
    CFamily,    // rs, ts, tsx, js, jsx, mjs, cjs, go, java, kt, swift, c, cpp, h, hpp, cc
    Python,     // py
    PowerShell, // ps1, psm1
    Shell,      // sh, bash
    Fallback,   // unknown extension — best-effort generic
}

impl Lexer {
    fn name(&self) -> &'static str {
        match self {
            Lexer::CFamily => "c_family",
            Lexer::Python => "python",
            Lexer::PowerShell => "powershell",
            Lexer::Shell => "shell",
            Lexer::Fallback => "fallback",
        }
    }
}

fn lexer_for_ext(ext: &str) -> Lexer {
    match ext {
        "rs" | "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs"
        | "go" | "java" | "kt" | "swift"
        | "c" | "h" | "cpp" | "hpp" | "cc" => Lexer::CFamily,
        "py" => Lexer::Python,
        "ps1" | "psm1" => Lexer::PowerShell,
        "sh" | "bash" => Lexer::Shell,
        _ => Lexer::Fallback,
    }
}

fn tokenize(src: &str, ext: &str) -> Vec<Token> {
    match lexer_for_ext(ext) {
        Lexer::CFamily => tokenize_generic(src, &["//"], true, true),
        Lexer::Python => tokenize_python(src),
        Lexer::PowerShell => tokenize_powershell(src),
        Lexer::Shell => tokenize_generic(src, &["#"], false, true),
        Lexer::Fallback => tokenize_generic(src, &["//", "#"], true, true),
    }
}

// ============================================================================
// Generic tokenizer (C-family, shell)
// ============================================================================

const TWO_CHAR_PUNCT: &[&[u8]] = &[
    b"==", b"!=", b"<=", b">=", b"&&", b"||", b"<<", b">>",
    b"->", b"=>", b"::", b"++", b"--", b"+=", b"-=", b"*=", b"/=", b"%=",
    b"&=", b"|=", b"^=", b"??", b"?.", b"..",
];
const THREE_CHAR_PUNCT: &[&[u8]] = &[
    b"...", b"..=", b"<<=", b">>=", b"===", b"!==", b"&&=", b"||=",
];

fn hash_punct_bytes(b: &[u8]) -> u32 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    b.hash(&mut h);
    h.finish() as u32
}

/// Generic tokenizer with configurable line-comment prefixes, block-comment support,
/// and standard string-quote handling.
fn tokenize_generic(
    src: &str,
    line_comment_prefixes: &[&str],
    has_block_comment: bool,
    has_backtick_string: bool,
) -> Vec<Token> {
    let bytes = src.as_bytes();
    let mut tokens = Vec::new();
    let mut line: u32 = 1;
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];

        if b == b'\n' {
            line += 1;
            i += 1;
            continue;
        }
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }

        // Block comment /* ... */
        if has_block_comment && b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                if bytes[i] == b'\n' {
                    line += 1;
                }
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            continue;
        }

        // Line comment
        let mut comment_match = None;
        for prefix in line_comment_prefixes {
            let pb = prefix.as_bytes();
            if bytes[i..].starts_with(pb) {
                comment_match = Some(pb.len());
                break;
            }
        }
        if let Some(plen) = comment_match {
            i += plen;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }

        // String literals: ", '
        if b == b'"' || b == b'\'' || (has_backtick_string && b == b'`') {
            let quote = b;
            tokens.push(Token { kind: TokenKind::Str, line });
            i += 1;
            while i < bytes.len() && bytes[i] != quote {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    if bytes[i + 1] == b'\n' {
                        line += 1;
                    }
                    i += 2;
                    continue;
                }
                if bytes[i] == b'\n' {
                    line += 1;
                }
                i += 1;
            }
            if i < bytes.len() {
                i += 1;
            }
            continue;
        }

        if b.is_ascii_digit() {
            tokens.push(Token { kind: TokenKind::Num, line });
            i += 1;
            while i < bytes.len() {
                let c = bytes[i];
                let is_numeric_char = c.is_ascii_digit()
                    || c == b'.'
                    || c == b'_'
                    || c.is_ascii_hexdigit()
                    || c == b'x'
                    || c == b'X';
                if is_numeric_char {
                    i += 1;
                } else if (c == b'+' || c == b'-')
                    && i > 0
                    && (bytes[i - 1] == b'e' || bytes[i - 1] == b'E')
                {
                    i += 1;
                } else {
                    break;
                }
            }
            continue;
        }

        if b.is_ascii_alphabetic() || b == b'_' || b == b'$' {
            tokens.push(Token { kind: TokenKind::Id, line });
            i += 1;
            while i < bytes.len() {
                let c = bytes[i];
                if c.is_ascii_alphanumeric() || c == b'_' {
                    i += 1;
                } else {
                    break;
                }
            }
            continue;
        }

        if i + 2 < bytes.len() {
            let three = &bytes[i..i + 3];
            if THREE_CHAR_PUNCT.contains(&three) {
                tokens.push(Token { kind: TokenKind::PunctHash(hash_punct_bytes(three)), line });
                i += 3;
                continue;
            }
        }
        if i + 1 < bytes.len() {
            let two = &bytes[i..i + 2];
            if TWO_CHAR_PUNCT.contains(&two) {
                tokens.push(Token { kind: TokenKind::PunctHash(hash_punct_bytes(two)), line });
                i += 2;
                continue;
            }
        }

        tokens.push(Token { kind: TokenKind::Punct(b), line });
        i += 1;
    }

    tokens
}

// ============================================================================
// Python tokenizer (handles triple-quoted strings)
// ============================================================================

fn tokenize_python(src: &str) -> Vec<Token> {
    let bytes = src.as_bytes();
    let mut tokens = Vec::new();
    let mut line: u32 = 1;
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];

        if b == b'\n' {
            line += 1;
            i += 1;
            continue;
        }
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }

        if b == b'#' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }

        // Triple-quoted string: """...""" or '''...'''
        if (b == b'"' || b == b'\'')
            && i + 2 < bytes.len()
            && bytes[i + 1] == b
            && bytes[i + 2] == b
        {
            let quote = b;
            tokens.push(Token { kind: TokenKind::Str, line });
            i += 3;
            while i + 2 < bytes.len()
                && !(bytes[i] == quote && bytes[i + 1] == quote && bytes[i + 2] == quote)
            {
                if bytes[i] == b'\n' {
                    line += 1;
                }
                i += 1;
            }
            i = (i + 3).min(bytes.len());
            continue;
        }

        if b == b'"' || b == b'\'' {
            let quote = b;
            tokens.push(Token { kind: TokenKind::Str, line });
            i += 1;
            while i < bytes.len() && bytes[i] != quote {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    if bytes[i + 1] == b'\n' {
                        line += 1;
                    }
                    i += 2;
                    continue;
                }
                if bytes[i] == b'\n' {
                    line += 1;
                }
                i += 1;
            }
            if i < bytes.len() {
                i += 1;
            }
            continue;
        }

        if b.is_ascii_digit() {
            tokens.push(Token { kind: TokenKind::Num, line });
            i += 1;
            while i < bytes.len() {
                let c = bytes[i];
                if c.is_ascii_digit() || c == b'.' || c == b'_' || c == b'j' || c == b'J' {
                    i += 1;
                } else {
                    break;
                }
            }
            continue;
        }

        if b.is_ascii_alphabetic() || b == b'_' {
            tokens.push(Token { kind: TokenKind::Id, line });
            i += 1;
            while i < bytes.len() {
                let c = bytes[i];
                if c.is_ascii_alphanumeric() || c == b'_' {
                    i += 1;
                } else {
                    break;
                }
            }
            continue;
        }

        if i + 1 < bytes.len() {
            let two = &bytes[i..i + 2];
            if TWO_CHAR_PUNCT.contains(&two) {
                tokens.push(Token { kind: TokenKind::PunctHash(hash_punct_bytes(two)), line });
                i += 2;
                continue;
            }
        }
        tokens.push(Token { kind: TokenKind::Punct(b), line });
        i += 1;
    }

    tokens
}

// ============================================================================
// PowerShell tokenizer (handles `<# ... #>` block comment, here-strings)
// ============================================================================

fn tokenize_powershell(src: &str) -> Vec<Token> {
    let bytes = src.as_bytes();
    let mut tokens = Vec::new();
    let mut line: u32 = 1;
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];

        if b == b'\n' {
            line += 1;
            i += 1;
            continue;
        }
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }

        // Block comment <# ... #>
        if b == b'<' && i + 1 < bytes.len() && bytes[i + 1] == b'#' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'#' && bytes[i + 1] == b'>') {
                if bytes[i] == b'\n' {
                    line += 1;
                }
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            continue;
        }

        if b == b'#' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }

        // Here-string: @" ... "@  or  @' ... '@
        if b == b'@'
            && i + 1 < bytes.len()
            && (bytes[i + 1] == b'"' || bytes[i + 1] == b'\'')
        {
            let quote = bytes[i + 1];
            tokens.push(Token { kind: TokenKind::Str, line });
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == quote && bytes[i + 1] == b'@') {
                if bytes[i] == b'\n' {
                    line += 1;
                }
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            continue;
        }

        if b == b'"' || b == b'\'' {
            let quote = b;
            tokens.push(Token { kind: TokenKind::Str, line });
            i += 1;
            while i < bytes.len() && bytes[i] != quote {
                if bytes[i] == b'`' && i + 1 < bytes.len() {
                    // PowerShell escape via backtick
                    if bytes[i + 1] == b'\n' {
                        line += 1;
                    }
                    i += 2;
                    continue;
                }
                if bytes[i] == b'\n' {
                    line += 1;
                }
                i += 1;
            }
            if i < bytes.len() {
                i += 1;
            }
            continue;
        }

        if b.is_ascii_digit() {
            tokens.push(Token { kind: TokenKind::Num, line });
            i += 1;
            while i < bytes.len() {
                let c = bytes[i];
                if c.is_ascii_digit() || c == b'.' || c == b'_' {
                    i += 1;
                } else {
                    break;
                }
            }
            continue;
        }

        // Identifier (incl. $variable)
        if b.is_ascii_alphabetic() || b == b'_' || b == b'$' || b == b'@' {
            tokens.push(Token { kind: TokenKind::Id, line });
            i += 1;
            while i < bytes.len() {
                let c = bytes[i];
                if c.is_ascii_alphanumeric() || c == b'_' || c == b':' && i + 1 < bytes.len() && bytes[i + 1] == b':' {
                    i += 1;
                } else {
                    break;
                }
            }
            continue;
        }

        if i + 1 < bytes.len() {
            let two = &bytes[i..i + 2];
            if TWO_CHAR_PUNCT.contains(&two) {
                tokens.push(Token { kind: TokenKind::PunctHash(hash_punct_bytes(two)), line });
                i += 2;
                continue;
            }
        }
        tokens.push(Token { kind: TokenKind::Punct(b), line });
        i += 1;
    }

    tokens
}

// ============================================================================
// Duplicate detection
// ============================================================================

type Pos = (usize, usize); // (file_idx, token_start_idx)

fn window_hash(file: &FileTokens, start: usize, w: usize) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for t in &file.tokens[start..start + w] {
        t.kind.hash(&mut h);
    }
    h.finish()
}

fn is_left_maximal(files: &[FileTokens], occurrences: &[Pos]) -> bool {
    if occurrences.iter().any(|&(_, s)| s == 0) {
        return true;
    }
    let first = occurrences[0];
    let prev_kind = files[first.0].tokens[first.1 - 1].kind;
    for &(fi, s) in &occurrences[1..] {
        if files[fi].tokens[s - 1].kind != prev_kind {
            return true;
        }
    }
    false
}

fn verify_window(files: &[FileTokens], occurrences: &[Pos], w: usize) -> Vec<Pos> {
    let first = occurrences[0];
    let ref_kinds: Vec<TokenKind> = files[first.0].tokens[first.1..first.1 + w]
        .iter()
        .map(|t| t.kind)
        .collect();
    occurrences
        .iter()
        .copied()
        .filter(|&(fi, s)| {
            files[fi].tokens[s..s + w]
                .iter()
                .zip(&ref_kinds)
                .all(|(t, k)| t.kind == *k)
        })
        .collect()
}

/// Extend the matched region right. Returns (total_len, gap_count).
/// match_tokens = total_len - gap_count.
/// When max_gap > 0, allows up to max_gap substitution gaps: a single token
/// can differ across occurrences if the next position again matches.
fn extend_right(
    files: &[FileTokens],
    occurrences: &[Pos],
    initial_w: usize,
    max_gap: usize,
) -> (usize, usize) {
    let mut w = initial_w;
    let mut gaps = 0usize;
    loop {
        let first = occurrences[0];
        let next_idx = first.1 + w;
        if next_idx >= files[first.0].tokens.len() {
            break;
        }
        let next_kind = files[first.0].tokens[next_idx].kind;
        let mut all_match = true;
        for &(fi, s) in &occurrences[1..] {
            let idx = s + w;
            if idx >= files[fi].tokens.len() || files[fi].tokens[idx].kind != next_kind {
                all_match = false;
                break;
            }
        }
        if all_match {
            w += 1;
            continue;
        }
        if gaps >= max_gap {
            break;
        }
        // Substitution gap: probe one position ahead — if all occurrences agree
        // there, accept (1 differing token, 1 matching token).
        let probe_idx = first.1 + w + 1;
        if probe_idx >= files[first.0].tokens.len() {
            break;
        }
        let probe_kind = files[first.0].tokens[probe_idx].kind;
        let mut all_probe = true;
        for &(fi, s) in &occurrences[1..] {
            let idx = s + w + 1;
            if idx >= files[fi].tokens.len() || files[fi].tokens[idx].kind != probe_kind {
                all_probe = false;
                break;
            }
        }
        if all_probe {
            w += 2;
            gaps += 1;
            continue;
        }
        break;
    }
    (w, gaps)
}

fn build_preview(file: &FileTokens, start_line: u32, end_line: u32) -> String {
    let lines: Vec<&str> = file.src.lines().collect();
    let s = (start_line as usize).saturating_sub(1);
    let e = (end_line as usize).min(lines.len());
    if s >= e {
        return String::new();
    }
    const MAX_PREVIEW_LINES: usize = 6;
    let actual_e = (s + MAX_PREVIEW_LINES).min(e);
    let mut preview = lines[s..actual_e].join("\n");
    if e > actual_e {
        preview.push_str(&format!("\n... ({} more lines)", e - actual_e));
    }
    preview
}

fn build_clusters(
    files: &[FileTokens],
    min_tokens: usize,
    min_lines: usize,
    max_gap: usize,
) -> Vec<Cluster> {
    // Per-file window hashing in parallel, then merge.
    let per_file: Vec<HashMap<u64, Vec<usize>>> = files
        .par_iter()
        .map(|f| {
            let mut local: HashMap<u64, Vec<usize>> = HashMap::new();
            if f.tokens.len() >= min_tokens {
                for start in 0..=(f.tokens.len() - min_tokens) {
                    let key = window_hash(f, start, min_tokens);
                    local.entry(key).or_default().push(start);
                }
            }
            local
        })
        .collect();

    let mut groups: HashMap<u64, Vec<Pos>> = HashMap::new();
    for (fi, fg) in per_file.iter().enumerate() {
        for (k, starts) in fg {
            let entry = groups.entry(*k).or_default();
            for &s in starts {
                entry.push((fi, s));
            }
        }
    }

    // Process groups in parallel.
    let group_vec: Vec<Vec<Pos>> = groups
        .into_iter()
        .filter_map(|(_, v)| if v.len() >= 2 { Some(v) } else { None })
        .collect();

    let mut clusters: Vec<Cluster> = group_vec
        .par_iter()
        .filter_map(|occs| {
            if !is_left_maximal(files, occs) {
                return None;
            }
            let valid = verify_window(files, occs, min_tokens);
            if valid.len() < 2 {
                return None;
            }
            let (ext_len, gaps) = extend_right(files, &valid, min_tokens, max_gap);
            let match_tokens = ext_len - gaps;

            let occurrences: Vec<Occurrence> = valid
                .iter()
                .map(|&(fi, s)| {
                    let f = &files[fi];
                    let start_line = f.tokens[s].line;
                    let end_line = f.tokens[s + ext_len - 1].line;
                    Occurrence { file_idx: fi, start_line, end_line }
                })
                .collect();

            let line_count = (occurrences[0].end_line - occurrences[0].start_line + 1) as usize;
            if line_count < min_lines {
                return None;
            }

            let first_occ = &occurrences[0];
            let preview =
                build_preview(&files[first_occ.file_idx], first_occ.start_line, first_occ.end_line);

            Some(Cluster {
                token_count: ext_len,
                match_tokens,
                gap_tokens: gaps,
                line_count,
                occurrences,
                preview,
            })
        })
        .collect();

    // Sort: prefer longer exact match, then more occurrences, then fewer gaps.
    clusters.sort_by(|a, b| {
        b.match_tokens
            .cmp(&a.match_tokens)
            .then_with(|| b.occurrences.len().cmp(&a.occurrences.len()))
            .then_with(|| a.gap_tokens.cmp(&b.gap_tokens))
    });
    clusters
}

// ============================================================================
// Output
// ============================================================================

fn display_path(p: &Path, root: &Path) -> String {
    if let Ok(rel) = p.strip_prefix(root) {
        rel.to_string_lossy().replace('\\', "/")
    } else {
        p.to_string_lossy().replace('\\', "/")
    }
}

fn cluster_to_json(c: &Cluster, files: &[FileTokens], root: &Path) -> Value {
    let match_type = if c.gap_tokens == 0 { "exact" } else { "type3" };
    json!({
        "match_type": match_type,
        "token_count": c.token_count,
        "match_tokens": c.match_tokens,
        "gap_tokens": c.gap_tokens,
        "line_count": c.line_count,
        "occurrence_count": c.occurrences.len(),
        "occurrences": c.occurrences.iter().map(|o| json!({
            "file": display_path(&files[o.file_idx].path, root),
            "start_line": o.start_line,
            "end_line": o.end_line,
        })).collect::<Vec<_>>(),
        "preview": c.preview,
    })
}

#[derive(Default)]
struct LangStat {
    files: u32,
    tokens: u64,
    lexer: &'static str,
}

fn build_language_breakdown(files: &[FileTokens]) -> Value {
    let mut by_ext: BTreeMap<String, LangStat> = BTreeMap::new();
    for f in files {
        let s = by_ext.entry(f.ext.clone()).or_default();
        s.files += 1;
        s.tokens += f.tokens.len() as u64;
        s.lexer = f.lexer;
    }
    let mut arr: Vec<Value> = by_ext
        .into_iter()
        .map(|(ext, s)| {
            json!({
                "ext": ext,
                "lexer": s.lexer,
                "files": s.files,
                "tokens": s.tokens,
            })
        })
        .collect();
    // Sort by tokens desc
    arr.sort_by(|a, b| {
        let at = a.get("tokens").and_then(|x| x.as_u64()).unwrap_or(0);
        let bt = b.get("tokens").and_then(|x| x.as_u64()).unwrap_or(0);
        bt.cmp(&at)
    });
    Value::Array(arr)
}

fn build_fallback_warnings(files: &[FileTokens]) -> Value {
    let mut by_ext: BTreeMap<String, u32> = BTreeMap::new();
    for f in files {
        if f.lexer == Lexer::Fallback.name() {
            *by_ext.entry(f.ext.clone()).or_default() += 1;
        }
    }
    let arr: Vec<Value> = by_ext
        .into_iter()
        .map(|(ext, count)| json!({"ext": ext, "files": count}))
        .collect();
    Value::Array(arr)
}

pub fn run(args: &[String]) {
    let parsed = parse_args(args);

    let paths = walk_files(&parsed.path, &parsed.exts, &parsed.excludes, parsed.no_gitignore);

    // Parallel read+tokenize.
    let files: Vec<FileTokens> = paths
        .par_iter()
        .filter_map(|p| {
            let src = fs::read_to_string(p).ok()?;
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_string();
            let lexer = lexer_for_ext(&ext).name();
            let tokens = tokenize(&src, &ext);
            Some(FileTokens { path: p.clone(), tokens, src, ext, lexer })
        })
        .collect();

    let total_tokens: usize = files.iter().map(|f| f.tokens.len()).sum();
    let language_breakdown = build_language_breakdown(&files);
    let fallback_warnings = build_fallback_warnings(&files);

    let clusters = build_clusters(&files, parsed.min_tokens, parsed.min_lines, parsed.gap);
    let total_clusters = clusters.len();
    let type3_count = clusters.iter().filter(|c| c.gap_tokens > 0).count();
    let shown: Vec<&Cluster> = clusters.iter().take(parsed.top).collect();

    let result = json!({
        "scanned_files": files.len(),
        "scanned_tokens": total_tokens,
        "min_tokens": parsed.min_tokens,
        "min_lines": parsed.min_lines,
        "max_gap": parsed.gap,
        "language_breakdown": language_breakdown,
        "fallback_lexer_used": fallback_warnings,
        "total_clusters": total_clusters,
        "type3_clusters": type3_count,
        "shown_clusters": shown.len(),
        "clusters": shown.iter().map(|c| cluster_to_json(c, &files, &parsed.path)).collect::<Vec<_>>(),
    });

    println!("{}", serde_json::to_string_pretty(&result).unwrap());
}
