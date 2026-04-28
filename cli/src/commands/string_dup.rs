//! String literal duplication scanner. Extracts string literals from source
//! files and groups identical (whitespace-normalized) or — opt-in — Jaccard-
//! similar contents. Useful for spotting repeated Tailwind class strings,
//! SQL fragments, prompts, URLs, etc.

use rayon::prelude::*;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_MIN_CHARS: usize = 30;
const DEFAULT_MIN_WORDS: usize = 3;
const DEFAULT_MIN_OCCURRENCES: usize = 2;
const DEFAULT_TOP: usize = 50;
const DEFAULT_JACCARD_THRESHOLD: f64 = 0.7;

const DEFAULT_EXTS: &[&str] = &[
    "rs", "ts", "tsx", "js", "jsx", "mjs", "cjs",
    "py", "ps1", "psm1", "sh", "bash",
    "go", "java", "kt", "swift",
    "c", "h", "cpp", "hpp", "cc",
    "html", "vue", "svelte", "astro",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LangKind {
    CFamily,
    Python,
    PowerShell,
    Shell,
    Markup, // html, vue, svelte, astro — strip <!-- --> + extract attribute strings
    Fallback,
}

fn lang_for_ext(ext: &str) -> LangKind {
    match ext {
        "rs" | "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs"
        | "go" | "java" | "kt" | "swift"
        | "c" | "h" | "cpp" | "hpp" | "cc" => LangKind::CFamily,
        "py" => LangKind::Python,
        "ps1" | "psm1" => LangKind::PowerShell,
        "sh" | "bash" => LangKind::Shell,
        "html" | "vue" | "svelte" | "astro" => LangKind::Markup,
        _ => LangKind::Fallback,
    }
}

fn lang_name(kind: LangKind) -> &'static str {
    match kind {
        LangKind::CFamily => "c_family",
        LangKind::Python => "python",
        LangKind::PowerShell => "powershell",
        LangKind::Shell => "shell",
        LangKind::Markup => "markup",
        LangKind::Fallback => "fallback",
    }
}

#[derive(Debug, Clone)]
struct StringLit {
    content: String, // whitespace-normalized
    line: u32,
}

struct FileStrings {
    path: PathBuf,
    strings: Vec<StringLit>,
    ext: String,
    lang: LangKind,
}

struct Args {
    path: PathBuf,
    exts: Vec<String>,
    min_chars: usize,
    min_words: usize,
    min_occurrences: usize,
    top: usize,
    similarity: Option<String>,
    threshold: f64,
    excludes: Vec<String>,
    no_gitignore: bool,
}

fn parse_args(args: &[String]) -> Args {
    let mut path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut exts: Vec<String> = DEFAULT_EXTS.iter().map(|s| s.to_string()).collect();
    let mut min_chars = DEFAULT_MIN_CHARS;
    let mut min_words = DEFAULT_MIN_WORDS;
    let mut min_occurrences = DEFAULT_MIN_OCCURRENCES;
    let mut top = DEFAULT_TOP;
    let mut similarity: Option<String> = None;
    let mut threshold = DEFAULT_JACCARD_THRESHOLD;
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
            "--min-chars" => {
                if i + 1 < args.len() {
                    min_chars = args[i + 1].parse().unwrap_or(DEFAULT_MIN_CHARS);
                    i += 1;
                }
            }
            "--min-words" => {
                if i + 1 < args.len() {
                    min_words = args[i + 1].parse().unwrap_or(DEFAULT_MIN_WORDS);
                    i += 1;
                }
            }
            "--min-occurrences" => {
                if i + 1 < args.len() {
                    min_occurrences = args[i + 1].parse().unwrap_or(DEFAULT_MIN_OCCURRENCES);
                    i += 1;
                }
            }
            "--top" => {
                if i + 1 < args.len() {
                    top = args[i + 1].parse().unwrap_or(DEFAULT_TOP);
                    i += 1;
                }
            }
            "--similarity" => {
                if i + 1 < args.len() {
                    similarity = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            "--threshold" => {
                if i + 1 < args.len() {
                    threshold = args[i + 1].parse().unwrap_or(DEFAULT_JACCARD_THRESHOLD);
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

    Args {
        path, exts, min_chars, min_words, min_occurrences,
        top, similarity, threshold, excludes, no_gitignore,
    }
}

// ============================================================================
// File discovery
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
// String extraction (comment-aware)
// ============================================================================

/// Normalize whitespace: collapse runs of \s+ into a single space, trim ends.
fn normalize_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_ws = true; // suppress leading whitespace
    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_ws {
                out.push(' ');
                prev_ws = true;
            }
        } else {
            out.push(c);
            prev_ws = false;
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

fn extract_strings(src: &str, lang: LangKind) -> Vec<StringLit> {
    match lang {
        LangKind::CFamily | LangKind::Fallback => {
            extract_generic(src, &["//"], true, true, lang == LangKind::Fallback)
        }
        LangKind::Shell => extract_generic(src, &["#"], false, true, false),
        LangKind::Python => extract_python(src),
        LangKind::PowerShell => extract_powershell(src),
        LangKind::Markup => extract_markup(src),
    }
}

fn extract_generic(
    src: &str,
    line_comment_prefixes: &[&str],
    has_block_comment: bool,
    has_backtick_string: bool,
    fallback_extra_comments: bool,
) -> Vec<StringLit> {
    let mut prefixes_owned: Vec<&str> = line_comment_prefixes.to_vec();
    if fallback_extra_comments && !prefixes_owned.contains(&"#") {
        prefixes_owned.push("#");
    }
    let bytes = src.as_bytes();
    let mut out = Vec::new();
    let mut line: u32 = 1;
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];

        if b == b'\n' {
            line += 1;
            i += 1;
            continue;
        }
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
        let mut comment_match = None;
        for prefix in &prefixes_owned {
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

        if b == b'"' || b == b'\'' || (has_backtick_string && b == b'`') {
            let quote = b;
            let start_line = line;
            let content_start = i + 1;
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
            let content_end = i;
            if i < bytes.len() {
                i += 1;
            }
            push_string(&mut out, &src[content_start..content_end], start_line);
            continue;
        }

        i += 1;
    }
    out
}

fn extract_python(src: &str) -> Vec<StringLit> {
    let bytes = src.as_bytes();
    let mut out = Vec::new();
    let mut line: u32 = 1;
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\n' {
            line += 1;
            i += 1;
            continue;
        }
        if b == b'#' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        // Triple-quoted string
        if (b == b'"' || b == b'\'')
            && i + 2 < bytes.len()
            && bytes[i + 1] == b
            && bytes[i + 2] == b
        {
            let quote = b;
            let start_line = line;
            let content_start = i + 3;
            i += 3;
            while i + 2 < bytes.len()
                && !(bytes[i] == quote && bytes[i + 1] == quote && bytes[i + 2] == quote)
            {
                if bytes[i] == b'\n' {
                    line += 1;
                }
                i += 1;
            }
            let content_end = i;
            i = (i + 3).min(bytes.len());
            push_string(&mut out, &src[content_start..content_end], start_line);
            continue;
        }
        if b == b'"' || b == b'\'' {
            let quote = b;
            let start_line = line;
            let content_start = i + 1;
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
            let content_end = i;
            if i < bytes.len() {
                i += 1;
            }
            push_string(&mut out, &src[content_start..content_end], start_line);
            continue;
        }
        i += 1;
    }
    out
}

fn extract_powershell(src: &str) -> Vec<StringLit> {
    let bytes = src.as_bytes();
    let mut out = Vec::new();
    let mut line: u32 = 1;
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\n' {
            line += 1;
            i += 1;
            continue;
        }
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
        // Here-string: @"..."@ or @'...'@
        if b == b'@'
            && i + 1 < bytes.len()
            && (bytes[i + 1] == b'"' || bytes[i + 1] == b'\'')
        {
            let quote = bytes[i + 1];
            let start_line = line;
            let content_start = i + 2;
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == quote && bytes[i + 1] == b'@') {
                if bytes[i] == b'\n' {
                    line += 1;
                }
                i += 1;
            }
            let content_end = i;
            i = (i + 2).min(bytes.len());
            push_string(&mut out, &src[content_start..content_end], start_line);
            continue;
        }
        if b == b'"' || b == b'\'' {
            let quote = b;
            let start_line = line;
            let content_start = i + 1;
            i += 1;
            while i < bytes.len() && bytes[i] != quote {
                if bytes[i] == b'`' && i + 1 < bytes.len() {
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
            let content_end = i;
            if i < bytes.len() {
                i += 1;
            }
            push_string(&mut out, &src[content_start..content_end], start_line);
            continue;
        }
        i += 1;
    }
    out
}

/// Markup: skip `<!-- ... -->`, extract attribute values `attr="..."` or `attr='...'`.
fn extract_markup(src: &str) -> Vec<StringLit> {
    let bytes = src.as_bytes();
    let mut out = Vec::new();
    let mut line: u32 = 1;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\n' {
            line += 1;
            i += 1;
            continue;
        }
        if bytes[i..].starts_with(b"<!--") {
            i += 4;
            while i + 2 < bytes.len() && !bytes[i..].starts_with(b"-->") {
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
            let start_line = line;
            let content_start = i + 1;
            i += 1;
            while i < bytes.len() && bytes[i] != quote {
                if bytes[i] == b'\n' {
                    line += 1;
                }
                i += 1;
            }
            let content_end = i;
            if i < bytes.len() {
                i += 1;
            }
            push_string(&mut out, &src[content_start..content_end], start_line);
            continue;
        }
        i += 1;
    }
    out
}

fn push_string(out: &mut Vec<StringLit>, raw: &str, line: u32) {
    let normalized = normalize_ws(raw);
    if normalized.is_empty() {
        return;
    }
    out.push(StringLit { content: normalized, line });
}

// ============================================================================
// Variant grouping (exact match)
// ============================================================================

#[derive(Debug, Clone)]
struct Occurrence {
    file_idx: usize,
    line: u32,
}

#[derive(Debug, Clone)]
struct Variant {
    content: String,
    char_count: usize,
    word_count: usize,
    occurrences: Vec<Occurrence>,
}

fn build_variants(files: &[FileStrings]) -> Vec<Variant> {
    let mut by_content: HashMap<String, Vec<Occurrence>> = HashMap::new();
    for (fi, f) in files.iter().enumerate() {
        for s in &f.strings {
            by_content
                .entry(s.content.clone())
                .or_default()
                .push(Occurrence { file_idx: fi, line: s.line });
        }
    }
    by_content
        .into_iter()
        .map(|(content, occurrences)| {
            let char_count = content.chars().count();
            let word_count = content.split_ascii_whitespace().count();
            Variant { content, char_count, word_count, occurrences }
        })
        .collect()
}

// ============================================================================
// Jaccard similarity clustering (opt-in)
// ============================================================================

fn word_set(s: &str) -> Vec<String> {
    let mut v: Vec<String> = s
        .split_ascii_whitespace()
        .map(|w| w.to_string())
        .collect();
    v.sort();
    v.dedup();
    v
}

fn jaccard(a: &[String], b: &[String]) -> f64 {
    let sa: HashSet<&str> = a.iter().map(|s| s.as_str()).collect();
    let sb: HashSet<&str> = b.iter().map(|s| s.as_str()).collect();
    let inter = sa.intersection(&sb).count();
    let union = sa.union(&sb).count();
    if union == 0 { 0.0 } else { inter as f64 / union as f64 }
}

/// Union-find for clustering Jaccard-similar variants.
struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self { Self { parent: (0..n).collect() } }
    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] == x { return x; }
        let p = self.find(self.parent[x]);
        self.parent[x] = p;
        p
    }
    fn union(&mut self, a: usize, b: usize) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra != rb { self.parent[ra] = rb; }
    }
}

fn build_similar_clusters(variants: &[Variant], threshold: f64) -> Vec<Vec<usize>> {
    let n = variants.len();
    if n < 2 { return Vec::new(); }
    let word_sets: Vec<Vec<String>> = variants.iter().map(|v| word_set(&v.content)).collect();

    // Inverted index: word -> Vec<variant_idx>
    let mut inverted: HashMap<String, Vec<usize>> = HashMap::new();
    for (idx, ws) in word_sets.iter().enumerate() {
        for w in ws {
            inverted.entry(w.clone()).or_default().push(idx);
        }
    }

    let mut uf = UnionFind::new(n);

    // For each variant, compute candidates (those sharing >= 1 word).
    // Process in parallel and collect edges to union sequentially.
    let edges: Vec<(usize, usize)> = (0..n).into_par_iter().flat_map(|i| {
        let mut local_edges: Vec<(usize, usize)> = Vec::new();
        let mut seen: HashSet<usize> = HashSet::new();
        for w in &word_sets[i] {
            if let Some(list) = inverted.get(w) {
                for &j in list {
                    if j > i && seen.insert(j) {
                        let s = jaccard(&word_sets[i], &word_sets[j]);
                        if s >= threshold {
                            local_edges.push((i, j));
                        }
                    }
                }
            }
        }
        local_edges
    }).collect();

    for (a, b) in edges {
        uf.union(a, b);
    }

    // Group by root
    let mut by_root: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        let r = uf.find(i);
        by_root.entry(r).or_default().push(i);
    }
    by_root.into_values().filter(|g| g.len() >= 2).collect()
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

fn truncate(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars { return s.to_string(); }
    let mut t: String = chars.iter().take(max_chars).collect();
    t.push_str("...");
    t
}

fn occurrence_to_json(o: &Occurrence, files: &[FileStrings], root: &Path) -> Value {
    json!({
        "file": display_path(&files[o.file_idx].path, root),
        "line": o.line,
    })
}

fn exact_cluster_to_json(v: &Variant, files: &[FileStrings], root: &Path) -> Value {
    json!({
        "type": "exact",
        "char_count": v.char_count,
        "word_count": v.word_count,
        "occurrence_count": v.occurrences.len(),
        "content": truncate(&v.content, 240),
        "occurrences": v.occurrences.iter().map(|o| occurrence_to_json(o, files, root)).collect::<Vec<_>>(),
    })
}

fn similar_cluster_to_json(
    indices: &[usize],
    variants: &[Variant],
    files: &[FileStrings],
    root: &Path,
) -> Value {
    let total_occ: usize = indices.iter().map(|&i| variants[i].occurrences.len()).sum();
    let max_chars = indices.iter().map(|&i| variants[i].char_count).max().unwrap_or(0);
    let max_words = indices.iter().map(|&i| variants[i].word_count).max().unwrap_or(0);

    let members: Vec<Value> = indices.iter().map(|&i| {
        let v = &variants[i];
        json!({
            "content": truncate(&v.content, 240),
            "char_count": v.char_count,
            "word_count": v.word_count,
            "occurrence_count": v.occurrences.len(),
            "occurrences": v.occurrences.iter().map(|o| occurrence_to_json(o, files, root)).collect::<Vec<_>>(),
        })
    }).collect();

    json!({
        "type": "similar",
        "variant_count": indices.len(),
        "total_occurrences": total_occ,
        "max_char_count": max_chars,
        "max_word_count": max_words,
        "members": members,
    })
}

fn build_language_breakdown(files: &[FileStrings]) -> Value {
    let mut by_ext: BTreeMap<String, (LangKind, u32, u64)> = BTreeMap::new();
    for f in files {
        let entry = by_ext.entry(f.ext.clone()).or_insert((f.lang, 0, 0));
        entry.0 = f.lang;
        entry.1 += 1;
        entry.2 += f.strings.len() as u64;
    }
    let mut arr: Vec<Value> = by_ext
        .into_iter()
        .map(|(ext, (lang, fcount, scount))| json!({
            "ext": ext,
            "lang": lang_name(lang),
            "files": fcount,
            "strings": scount,
        }))
        .collect();
    arr.sort_by(|a, b| {
        let av = a.get("strings").and_then(|x| x.as_u64()).unwrap_or(0);
        let bv = b.get("strings").and_then(|x| x.as_u64()).unwrap_or(0);
        bv.cmp(&av)
    });
    Value::Array(arr)
}

fn build_fallback_warnings(files: &[FileStrings]) -> Value {
    let mut by_ext: BTreeMap<String, u32> = BTreeMap::new();
    for f in files {
        if f.lang == LangKind::Fallback {
            *by_ext.entry(f.ext.clone()).or_default() += 1;
        }
    }
    Value::Array(
        by_ext.into_iter()
            .map(|(ext, count)| json!({"ext": ext, "files": count}))
            .collect(),
    )
}

pub fn run(args: &[String]) {
    let parsed = parse_args(args);

    let paths = walk_files(
        &parsed.path,
        &parsed.exts,
        &parsed.excludes,
        parsed.no_gitignore,
    );

    let files: Vec<FileStrings> = paths
        .par_iter()
        .filter_map(|p| {
            let src = fs::read_to_string(p).ok()?;
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_string();
            let lang = lang_for_ext(&ext);
            let mut strings = extract_strings(&src, lang);
            // filter by min chars / words right at extraction
            strings.retain(|s| {
                s.content.chars().count() >= parsed.min_chars
                    && s.content.split_ascii_whitespace().count() >= parsed.min_words
            });
            Some(FileStrings { path: p.clone(), strings, ext, lang })
        })
        .collect();

    let extracted_strings: usize = files.iter().map(|f| f.strings.len()).sum();
    let language_breakdown = build_language_breakdown(&files);
    let fallback_warnings = build_fallback_warnings(&files);

    let variants = build_variants(&files);

    // Exact clusters: variants whose occurrences.len() >= min_occurrences.
    let mut exact: Vec<&Variant> = variants
        .iter()
        .filter(|v| v.occurrences.len() >= parsed.min_occurrences)
        .collect();
    exact.sort_by(|a, b| {
        b.occurrences
            .len()
            .cmp(&a.occurrences.len())
            .then_with(|| b.char_count.cmp(&a.char_count))
    });
    let total_exact = exact.len();
    let shown_exact: Vec<&&Variant> = exact.iter().take(parsed.top).collect();

    let mut result = json!({
        "scanned_files": files.len(),
        "extracted_strings": extracted_strings,
        "unique_variants": variants.len(),
        "min_chars": parsed.min_chars,
        "min_words": parsed.min_words,
        "min_occurrences": parsed.min_occurrences,
        "language_breakdown": language_breakdown,
        "fallback_lexer_used": fallback_warnings,
        "total_exact_clusters": total_exact,
        "shown_exact_clusters": shown_exact.len(),
        "exact_clusters": shown_exact.iter().map(|v| exact_cluster_to_json(v, &files, &parsed.path)).collect::<Vec<_>>(),
    });

    // Similar clusters (Jaccard) — opt-in
    if parsed.similarity.as_deref() == Some("jaccard") {
        let groups = build_similar_clusters(&variants, parsed.threshold);
        // Only keep clusters with >= min_occurrences total occurrences and at least 2 distinct variants.
        let mut groups_filtered: Vec<Vec<usize>> = groups
            .into_iter()
            .filter(|g| {
                g.len() >= 2
                    && g.iter().map(|&i| variants[i].occurrences.len()).sum::<usize>()
                        >= parsed.min_occurrences
            })
            .collect();
        // Sort: total occurrences desc, then variant count desc
        groups_filtered.sort_by(|a, b| {
            let total_a: usize = a.iter().map(|&i| variants[i].occurrences.len()).sum();
            let total_b: usize = b.iter().map(|&i| variants[i].occurrences.len()).sum();
            total_b.cmp(&total_a).then_with(|| b.len().cmp(&a.len()))
        });
        let total_similar = groups_filtered.len();
        let shown_similar: Vec<&Vec<usize>> = groups_filtered.iter().take(parsed.top).collect();

        let obj = result.as_object_mut().unwrap();
        obj.insert("similarity".into(), Value::String("jaccard".into()));
        obj.insert("threshold".into(), json!(parsed.threshold));
        obj.insert("total_similar_clusters".into(), json!(total_similar));
        obj.insert("shown_similar_clusters".into(), json!(shown_similar.len()));
        obj.insert(
            "similar_clusters".into(),
            Value::Array(
                shown_similar
                    .iter()
                    .map(|g| similar_cluster_to_json(g, &variants, &files, &parsed.path))
                    .collect(),
            ),
        );
    }

    println!("{}", serde_json::to_string_pretty(&result).unwrap());
}
