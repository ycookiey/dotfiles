//! Code duplication scanner. Token-normalized n-gram detection for refactor candidates.
//!
//! Walks source files under cwd (respecting .gitignore), normalizes
//! identifiers/literals/comments, and reports duplicate token sequences.

use rayon::prelude::*;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::scan_common::{self, display_path, walk_files, CodeKind, Event, Lang, Lexer, Syntax};

const DEFAULT_MIN_TOKENS: usize = 30;
const DEFAULT_MIN_LINES: usize = 3;
const DEFAULT_TOP: usize = 50;

pub(crate) const DEFAULT_EXTS: &[&str] = &[
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
    /// Index into MULTI_PUNCT.
    PunctIdx(u8),
}

impl TokenKind {
    /// A distinct nonzero integer per kind, the input to the window hash.
    fn code(self) -> u64 {
        match self {
            TokenKind::Id => 1,
            TokenKind::Num => 2,
            TokenKind::Str => 3,
            TokenKind::Punct(b) => 4 + b as u64,
            TokenKind::PunctIdx(i) => 260 + i as u64,
        }
    }
}

struct FileTokens {
    path: PathBuf,
    tokens: Vec<Token>,
    src: String,
    ext: String,
    lang: Lang,
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
// Tokenizing (scan_common::Lexer)
// ============================================================================

/// The syntax dup-scan lexes `ext` with. Markup files (html, vue, svelte, astro) keep the
/// Fallback rules, which also skip the `//` / `/* */` comments of their script blocks.
fn syntax_for_ext(ext: &str) -> Syntax {
    let mut syntax = Syntax::for_ext(ext);
    if syntax.lang == Lang::Markup {
        syntax.lang = Lang::Fallback;
    }
    syntax
}

/// Token kinds and lines of `src`. A Rust lifetime / label is `'` + Id, a Rust char
/// literal a Str like any string.
fn tokenize(src: &str, ext: &str) -> Vec<Token> {
    Lexer::new(src, syntax_for_ext(ext))
        .map(|ev| match ev {
            Event::Code { kind, line, .. } => {
                let kind = match kind {
                    CodeKind::Id => TokenKind::Id,
                    CodeKind::Num => TokenKind::Num,
                    CodeKind::Punct(b) => TokenKind::Punct(b),
                    CodeKind::MultiPunct(i) => TokenKind::PunctIdx(i),
                };
                Token { kind, line }
            }
            Event::Str { line, .. } => Token { kind: TokenKind::Str, line },
        })
        .collect()
}

// ============================================================================
// Duplicate detection
// ============================================================================

type Pos = (usize, usize); // (file_idx, token_start_idx)

/// Odd multiplier of the polynomial window hash.
const ROLL_BASE: u64 = 0x9E37_79B9_7F4A_7C15;

/// Calls `f(start, hash)` for every window of `w` tokens, in start order. The hash is the
/// polynomial sum of the token codes mod 2^64, rolled one token at a time, so all windows
/// cost O(tokens) instead of O(tokens * w). Equal windows get equal hashes; unequal windows
/// may collide, which callers resolve by comparing the tokens (split_equal_windows).
fn for_each_window_hash(tokens: &[Token], w: usize, mut f: impl FnMut(usize, u64)) {
    let n = tokens.len();
    if n < w {
        return;
    }
    if w == 0 {
        (0..=n).for_each(|s| f(s, 0));
        return;
    }
    // ROLL_BASE^(w-1): the weight of the token leaving the window.
    let top = (1..w).fold(1u64, |p, _| p.wrapping_mul(ROLL_BASE));
    let mut h = tokens[..w]
        .iter()
        .fold(0u64, |h, t| h.wrapping_mul(ROLL_BASE).wrapping_add(t.kind.code()));
    f(0, h);
    for s in 1..=(n - w) {
        h = h
            .wrapping_sub(tokens[s - 1].kind.code().wrapping_mul(top))
            .wrapping_mul(ROLL_BASE)
            .wrapping_add(tokens[s + w - 1].kind.code());
        f(s, h);
    }
}

/// True when the `w` tokens at `a` and at `b` have the same kinds.
fn same_window(files: &[FileTokens], a: Pos, b: Pos, w: usize) -> bool {
    let ta = &files[a.0].tokens[a.1..a.1 + w];
    let tb = &files[b.0].tokens[b.1..b.1 + w];
    ta.iter().zip(tb).all(|(x, y)| x.kind == y.kind)
}

/// Splits a run of windows with one hash into the classes of equal windows that have at
/// least 2 members. Each class keeps the run's (file, start) order. A run normally is one
/// class; a hash collision yields several instead of mixing different code in one group.
fn split_equal_windows(files: &[FileTokens], run: &[(u64, u32, u32)], w: usize) -> Vec<Vec<Pos>> {
    let mut rest: Vec<Pos> = run.iter().map(|&(_, fi, s)| (fi as usize, s as usize)).collect();
    let mut classes = Vec::new();
    while rest.len() >= 2 {
        let head = rest[0];
        let (same, other): (Vec<Pos>, Vec<Pos>) =
            rest.iter().partition(|&&p| same_window(files, head, p, w));
        if same.len() >= 2 {
            classes.push(same);
        }
        rest = other;
    }
    classes
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

/// The first MAX_PREVIEW_LINES lines of start_line..=end_line (1-based), then a count of the
/// rest. end_line is clamped to the file's line count. Reads only up to end_line instead of
/// splitting the whole file.
fn build_preview(file: &FileTokens, start_line: u32, end_line: u32) -> String {
    const MAX_PREVIEW_LINES: usize = 6;
    let s = (start_line as usize).saturating_sub(1);
    let e = end_line as usize;
    if s >= e {
        return String::new();
    }
    let mut lines = file.src.lines().skip(s);
    let shown: Vec<&str> = lines.by_ref().take(MAX_PREVIEW_LINES.min(e - s)).collect();
    if shown.is_empty() {
        return String::new();
    }
    let mut preview = shown.join("\n");
    // Lines left up to end_line, stopping at the end of the file.
    let more = lines.take(e - s - shown.len()).count();
    if more > 0 {
        preview.push_str(&format!("\n... ({} more lines)", more));
    }
    preview
}

fn build_clusters(
    files: &[FileTokens],
    min_tokens: usize,
    min_lines: usize,
    max_gap: usize,
) -> Vec<Cluster> {
    // Every window as (hash, file, start), hashed per file in parallel. Sorting the tuples
    // puts equal hashes in one run, each run in (file, start) order; extend_right and
    // is_left_maximal take the first occurrence as reference, so that order is fixed.
    let mut windows: Vec<(u64, u32, u32)> = files
        .par_iter()
        .enumerate()
        .flat_map_iter(|(fi, f)| {
            let fi = u32::try_from(fi).expect("dup-scan: more than u32::MAX files");
            let mut local = Vec::with_capacity((f.tokens.len() + 1).saturating_sub(min_tokens));
            for_each_window_hash(&f.tokens, min_tokens, |s, h| {
                let s = u32::try_from(s).expect("dup-scan: more than u32::MAX tokens in a file");
                local.push((h, fi, s));
            });
            local
        })
        .collect();
    windows.par_sort_unstable();

    // Runs of one window are unique code; skip them before any allocation.
    let runs: Vec<&[(u64, u32, u32)]> =
        windows.chunk_by(|a, b| a.0 == b.0).filter(|run| run.len() >= 2).collect();

    let mut clusters: Vec<Cluster> = runs
        .par_iter()
        .flat_map_iter(|run| split_equal_windows(files, run, min_tokens))
        .filter_map(|valid| {
            if !is_left_maximal(files, &valid) {
                return None;
            }
            let (ext_len, gaps) = extend_right(files, &valid, min_tokens, max_gap);
            let match_tokens = ext_len - gaps;

            let mut occurrences: Vec<Occurrence> = valid
                .iter()
                .map(|&(fi, s)| {
                    let f = &files[fi];
                    let start_line = f.tokens[s].line;
                    let end_line = f.tokens[s + ext_len - 1].line;
                    Occurrence { file_idx: fi, start_line, end_line }
                })
                .collect();

            // 同一ファイル内で重なる/隣接する occurrence を集約し、スライディングウィンドウ
            // による過剰カウントを除去する。連続領域の構文的繰り返し (JSX/StyleSheet 定型等)
            // は1件に畳まれ、離散したリテラル重複 (例: ['hp','atk',...]) は本来の出現数を保つ。
            occurrences.sort_by(|a, b| {
                a.file_idx.cmp(&b.file_idx).then(a.start_line.cmp(&b.start_line))
            });
            let mut merged: Vec<Occurrence> = Vec::with_capacity(occurrences.len());
            for occ in occurrences {
                if let Some(last) = merged.last_mut() {
                    if last.file_idx == occ.file_idx && occ.start_line <= last.end_line + 1 {
                        last.end_line = last.end_line.max(occ.end_line);
                        continue;
                    }
                }
                merged.push(occ);
            }
            let occurrences = merged;
            if occurrences.len() < 2 {
                return None;
            }

            let line_count = (occurrences[0].end_line - occurrences[0].start_line + 1) as usize;
            if line_count < min_lines {
                return None;
            }

            Some(Cluster {
                token_count: ext_len,
                match_tokens,
                gap_tokens: gaps,
                line_count,
                occurrences,
            })
        })
        .collect();

    // Sort by "saved tokens" = match_tokens × (occurrences - 1): the total token volume
    // removable by consolidating this duplicate. Surfaces both "short but high-frequency"
    // (e.g. ['hp','atk',...]) and "long but low-frequency" (large data defs) on the same
    // footing, while short+rare noise sinks. Tie-break: longer match, then fewer gaps,
    // then the clusters' contents (cluster_content_order), so equal clusters keep the same
    // order and the same --top cut across runs instead of following HashMap order.
    clusters.sort_by(|a, b| {
        let sa = a.match_tokens.saturating_mul(a.occurrences.len().saturating_sub(1));
        let sb = b.match_tokens.saturating_mul(b.occurrences.len().saturating_sub(1));
        sb.cmp(&sa)
            .then_with(|| b.match_tokens.cmp(&a.match_tokens))
            .then_with(|| a.gap_tokens.cmp(&b.gap_tokens))
            .then_with(|| cluster_content_order(files, a, b))
    });
    drop_subsumed(clusters)
}

/// Drops each cluster whose occurrences all lie inside the occurrences of one cluster kept
/// before it in rank order, i.e. a cluster that reports no file region the kept one does not
/// already report. Every window group that is left-maximal becomes its own cluster, so a
/// --gap match also yields the exact matches that start right after each gap, and periodic
/// code yields sub-ranges of the same region. Containment is checked against a single kept
/// cluster, never the union of several, so a cluster with an occurrence elsewhere (3
/// occurrences where only 2 match longer) stays. Expects `clusters` sorted by rank and each
/// cluster's occurrences sorted by (file_idx, start_line) and disjoint per file.
fn drop_subsumed(clusters: Vec<Cluster>) -> Vec<Cluster> {
    let mut kept: Vec<Cluster> = Vec::with_capacity(clusters.len());
    // file_idx -> (start_line, end_line, index into kept) of every kept occurrence.
    let mut spans_by_file: HashMap<usize, Vec<(u32, u32, usize)>> = HashMap::new();
    for c in clusters {
        let first = &c.occurrences[0];
        let subsumed = spans_by_file.get(&first.file_idx).is_some_and(|spans| {
            spans.iter().any(|&(start, end, k)| {
                start <= first.start_line && first.end_line <= end && occurrences_within(&c, &kept[k])
            })
        });
        if subsumed {
            continue;
        }
        let k = kept.len();
        for o in &c.occurrences {
            spans_by_file.entry(o.file_idx).or_default().push((o.start_line, o.end_line, k));
        }
        kept.push(c);
    }
    kept
}

/// True when every occurrence of `inner` lies inside some occurrence of `outer`. Several
/// occurrences of `inner` may share one occurrence of `outer`.
fn occurrences_within(inner: &Cluster, outer: &Cluster) -> bool {
    inner.occurrences.iter().all(|o| {
        // outer's occurrences are sorted and disjoint per file, so only the last one starting
        // at or before o can contain it.
        let idx = outer
            .occurrences
            .partition_point(|a| (a.file_idx, a.start_line) <= (o.file_idx, o.start_line));
        idx > 0 && {
            let a = &outer.occurrences[idx - 1];
            a.file_idx == o.file_idx && o.end_line <= a.end_line
        }
    })
}

/// Orders clusters by where they occur: the first occurrence's (path, start_line), then
/// the occurrence count, then the remaining occurrences and the token count. Paths are
/// compared rather than file indexes so the order does not depend on the walk order.
fn cluster_content_order(files: &[FileTokens], a: &Cluster, b: &Cluster) -> std::cmp::Ordering {
    let occ_key = |o: &Occurrence| (files[o.file_idx].path.as_path(), o.start_line, o.end_line);
    let first = |c: &Cluster| c.occurrences.first().map(|o| (files[o.file_idx].path.as_path(), o.start_line));
    first(a)
        .cmp(&first(b))
        .then_with(|| a.occurrences.len().cmp(&b.occurrences.len()))
        .then_with(|| a.occurrences.iter().map(occ_key).cmp(b.occurrences.iter().map(occ_key)))
        .then_with(|| a.token_count.cmp(&b.token_count))
}

// ============================================================================
// Output
// ============================================================================

/// Builds the preview here rather than in build_clusters, so only the clusters shown under
/// --top read their source lines.
fn cluster_to_json(c: &Cluster, files: &[FileTokens], root: &Path) -> Value {
    let match_type = if c.gap_tokens == 0 { "exact" } else { "type3" };
    let first = &c.occurrences[0];
    let preview = build_preview(&files[first.file_idx], first.start_line, first.end_line);
    json!({
        "match_type": match_type,
        "token_count": c.token_count,
        "match_tokens": c.match_tokens,
        "gap_tokens": c.gap_tokens,
        "line_count": c.line_count,
        "occurrence_count": c.occurrences.len(),
        "saved_tokens": c.match_tokens.saturating_mul(c.occurrences.len().saturating_sub(1)),
        "occurrences": c.occurrences.iter().map(|o| json!({
            "file": display_path(&files[o.file_idx].path, root),
            "start_line": o.start_line,
            "end_line": o.end_line,
        })).collect::<Vec<_>>(),
        "preview": preview,
    })
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
            let lang = syntax_for_ext(&ext).lang;
            let tokens = tokenize(&src, &ext);
            Some(FileTokens { path: p.clone(), tokens, src, ext, lang })
        })
        .collect();

    let total_tokens: usize = files.iter().map(|f| f.tokens.len()).sum();
    let language_breakdown = scan_common::language_breakdown(
        files.iter().map(|f| (f.ext.as_str(), f.lang, f.tokens.len() as u64)),
        "lexer",
        "tokens",
    );
    let fallback_warnings = scan_common::fallback_warnings(files.iter().map(|f| (f.ext.as_str(), f.lang)));

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

#[cfg(test)]
mod rust_quote_tests {
    use super::*;

    // Kept in a .txt file so `string-dup --ext rs` over cli/src does not report it.
    const FIXTURE: &str = include_str!("testdata/rust_quotes.txt");

    fn kinds(src: &str, ext: &str) -> Vec<TokenKind> {
        tokenize(src, ext).into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn rust_quote_dup_scan_tokens() {
        use TokenKind::{Id, Num, Punct, Str};
        let arrow = TokenKind::PunctIdx(scan_common::multi_punct(b"->").unwrap());
        let lifetime = [Punct(b'\''), Id];
        let mut expected = vec![Id, Id, Punct(b'<')];
        expected.extend(lifetime);
        expected.extend([Punct(b'>'), Punct(b'('), Id, Punct(b':'), Punct(b'&')]);
        expected.extend(lifetime);
        expected.extend([Id, Punct(b')'), arrow, Punct(b'&')]);
        expected.extend(lifetime);
        expected.extend([Id, Punct(b'{')]);
        expected.extend([Id, Id, Punct(b'='), Str, Punct(b';')]); // let c = 'x';
        expected.extend([Id, Id, Punct(b'='), Str, Punct(b';')]); // let e = '\n';
        expected.extend([Id, Id, Punct(b'='), Str, Punct(b';')]); // let r = r#"…"#;
        expected.extend([Str, Punct(b'}')]);
        let first_line = FIXTURE.lines().next().unwrap();
        assert_eq!(kinds(first_line, "rs"), expected);

        let second_line = FIXTURE.lines().nth(1).unwrap();
        assert_eq!(
            kinds(second_line, "rs"),
            vec![
                Id, Id, Punct(b'('), Punct(b')'), Punct(b'{'), Id, Id, Punct(b'='),
                Punct(b'['), Num, Punct(b','), Num, Punct(b','), Num, Punct(b']'),
                Punct(b';'), Punct(b'}'),
            ]
        );
    }

    #[test]
    fn rust_quote_raw_string_keeps_line_numbers() {
        let toks = tokenize("r#\"a\nb\"#\nx", "rs");
        let got: Vec<(TokenKind, u32)> = toks.iter().map(|t| (t.kind, t.line)).collect();
        assert_eq!(got, vec![(TokenKind::Str, 1), (TokenKind::Id, 3)]);
    }

    #[test]
    fn rust_quote_rules_do_not_apply_to_ts() {
        use TokenKind::{Id, Punct, Str};
        // In TS `'` always opens a string, even when it looks like a lifetime.
        assert_eq!(
            kinds("s = 'a b'; f<'x>", "ts"),
            vec![Id, Punct(b'='), Str, Punct(b';'), Id, Punct(b'<'), Str]
        );
        assert_eq!(kinds("r#\"x\"#", "ts"), vec![Id, Punct(b'#'), Str, Punct(b'#')]);
    }
}

#[cfg(test)]
mod subsumption_tests {
    use super::*;

    /// One JS statement whose punctuation follows the bits of `i`, so a body built from
    /// consecutive `i` has no repeating token-kind run (Id and Num collapse, so a body of
    /// same-shaped lines would match itself).
    fn line(i: u32, rhs: &str) -> String {
        let mut s = String::from("  v");
        for bit in 0..5 {
            s.push_str(if (i >> bit) & 1 == 1 { ".x" } else { "(x)" });
        }
        format!("{s} = {rhs};\n")
    }

    /// A 27-line function: header, statements 1..=25, closing brace. `rhs` gives each
    /// statement's right-hand side.
    fn function(rhs: impl Fn(u32) -> String) -> String {
        let mut src = String::from("function f() {\n");
        for i in 1..=25 {
            src.push_str(&line(i, &rhs(i)));
        }
        src.push_str("}\n");
        src
    }

    fn file(name: &str, src: String) -> FileTokens {
        let tokens = tokenize(&src, "js");
        FileTokens { path: PathBuf::from(name), tokens, src, ext: "js".into(), lang: Lang::CFamily }
    }

    /// A cluster's occurrences as "file_idx:start-end", space separated. Tests spell clusters
    /// as such strings so their tables do not show up in `dup-scan` over this file.
    fn occs(c: &Cluster) -> String {
        let spans: Vec<String> = c
            .occurrences
            .iter()
            .map(|o| format!("{}:{}-{}", o.file_idx, o.start_line, o.end_line))
            .collect();
        spans.join(" ")
    }

    /// Builds a cluster from the `occs` notation.
    fn cluster(spec: &str) -> Cluster {
        let occurrences = spec
            .split(' ')
            .map(|span| {
                let (file, lines) = span.split_once(':').unwrap();
                let (start, end) = lines.split_once('-').unwrap();
                Occurrence {
                    file_idx: file.parse().unwrap(),
                    start_line: start.parse().unwrap(),
                    end_line: end.parse().unwrap(),
                }
            })
            .collect();
        Cluster { token_count: 0, match_tokens: 0, gap_tokens: 0, line_count: 0, occurrences }
    }

    #[test]
    fn one_token_difference_reports_each_region_once() {
        // Files 0 and 1 differ in one token at the middle statement (line 14).
        let p = function(|i| i.to_string());
        let q = function(|i| if i == 13 { "\"s\"".to_string() } else { i.to_string() });
        let files = vec![file("p.js", p), file("q.js", q)];
        let clusters_for = |gap| {
            let mut got: Vec<_> = build_clusters(&files, 20, 1, gap).iter().map(occs).collect();
            got.sort();
            got
        };
        // Exact matches on either side of the difference; neither contains the other.
        assert_eq!(clusters_for(0), ["0:1-14 1:1-14", "0:14-27 1:14-27"]);
        // The gap match spans the function and absorbs the exact match after the gap.
        assert_eq!(clusters_for(2), ["0:1-27 1:1-27"]);
    }

    #[test]
    fn cluster_with_an_occurrence_outside_the_longer_match_stays() {
        // Files 0 and 1 hold the whole function; file 2 holds only its last statements.
        let whole = function(|i| i.to_string());
        let mut tail = String::new();
        for i in 18..=25 {
            tail.push_str(&line(i, &i.to_string()));
        }
        tail.push_str("}\n");
        let files = vec![file("a.js", whole.clone()), file("b.js", whole), file("c.js", tail)];
        // The 3-occurrence cluster ranks after the whole-function one and its occurrences in
        // files 0 and 1 lie inside it, so only the one in file 2 keeps it.
        let got: Vec<_> = build_clusters(&files, 20, 1, 0).iter().map(occs).collect();
        assert_eq!(got, ["0:1-27 1:1-27", "0:19-27 1:19-27 2:1-9"]);
    }

    #[test]
    fn drop_subsumed_checks_each_occurrence_against_one_kept_cluster() {
        let input = [
            // Inside the next cluster but ranked before it, so never checked against it: stays.
            "0:5-9 1:5-9",
            "0:1-27 1:1-27",
            // Two occurrences inside one occurrence of the cluster above: dropped.
            "0:2-13 0:15-26 1:2-26",
            // An occurrence in a file the whole-range cluster does not cover: stays.
            "0:2-10 1:2-10 2:5-13",
            // Runs past the whole-range cluster's end: stays.
            "0:20-30 1:20-27",
            // Inside the union of the two clusters above but not inside either one: stays.
            "1:5-25 2:6-12",
            // Inside the 3-occurrence cluster kept above: dropped.
            "1:3-9 2:5-13",
        ];
        let kept = drop_subsumed(input.iter().map(|s| cluster(s)).collect());
        let got: Vec<_> = kept.iter().map(occs).collect();
        let want: Vec<_> = [0, 1, 3, 4, 5].iter().map(|&i| input[i]).collect();
        assert_eq!(got, want);
    }
}

#[cfg(test)]
mod window_hash_tests {
    use super::*;

    fn file(src: &str) -> FileTokens {
        let tokens = tokenize(src, "js");
        FileTokens { path: PathBuf::new(), tokens, src: src.into(), ext: "js".into(), lang: Lang::CFamily }
    }

    #[test]
    fn rolled_hash_matches_hash_from_scratch() {
        let f = file("a.b(c, 1) === d->e ... 'x' + f[g] <<= h ?? i;");
        for w in 0..=f.tokens.len() + 1 {
            let mut got = Vec::new();
            for_each_window_hash(&f.tokens, w, |s, h| got.push((s, h)));
            let want: Vec<(usize, u64)> = (0..(f.tokens.len() + 1).saturating_sub(w))
                .map(|s| {
                    let h = f.tokens[s..s + w]
                        .iter()
                        .fold(0u64, |h, t| h.wrapping_mul(ROLL_BASE).wrapping_add(t.kind.code()));
                    (s, h)
                })
                .collect();
            assert_eq!(got, want, "w={w}");
        }
    }

    #[test]
    fn split_equal_windows_separates_colliding_windows() {
        // Windows of 3 tokens: "a + b" (Id Punct Id) at 0, 4 and 8 of file 0 and 0 of file 1;
        // "1 - 2" (Num Punct Num) at 12 and 16 of file 0; "c * d" once at 20.
        let files = vec![file("a + b;a + b;a + b;1 - 2;1 - 2;c * d;"), file("x + y;")];
        // As if all windows shared one hash, in (file, start) order.
        let run = [(7, 0, 0), (7, 0, 4), (7, 0, 8), (7, 0, 12), (7, 0, 16), (7, 0, 20), (7, 1, 0)];
        assert_eq!(
            split_equal_windows(&files, &run, 3),
            vec![vec![(0, 0), (0, 4), (0, 8), (1, 0)], vec![(0, 12), (0, 16)]]
        );
    }
}

#[cfg(test)]
mod preview_tests {
    use super::*;

    /// The preview as built by splitting the whole file into lines.
    fn reference(src: &str, start_line: u32, end_line: u32) -> String {
        let lines: Vec<&str> = src.lines().collect();
        let s = (start_line as usize).saturating_sub(1);
        let e = (end_line as usize).min(lines.len());
        if s >= e {
            return String::new();
        }
        let actual_e = (s + 6).min(e);
        let mut preview = lines[s..actual_e].join("\n");
        if e > actual_e {
            preview.push_str(&format!("\n... ({} more lines)", e - actual_e));
        }
        preview
    }

    #[test]
    fn matches_whole_file_split_for_every_range() {
        let numbered = |n: usize, eol: &str, last_eol: bool| {
            let mut s: String = (1..=n).map(|i| format!("line {i}{eol}")).collect();
            if !last_eol {
                s.truncate(s.len() - eol.len());
            }
            s
        };
        let sources = [
            String::new(),
            "\n".to_string(),
            numbered(3, "\n", true),
            numbered(12, "\n", false),
            numbered(12, "\r\n", true),
            "a\n\nb\r\n\r\nc".to_string(),
        ];
        for src in sources {
            let file = FileTokens {
                path: PathBuf::new(),
                tokens: Vec::new(),
                src: src.clone(),
                ext: String::new(),
                lang: Lang::Fallback,
            };
            for start in 0..16u32 {
                for end in 0..16u32 {
                    assert_eq!(
                        build_preview(&file, start, end),
                        reference(&src, start, end),
                        "src={src:?} start={start} end={end}"
                    );
                }
            }
        }
    }
}
