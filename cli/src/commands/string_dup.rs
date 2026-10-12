//! String literal duplication scanner. Extracts string literals from source
//! files and groups identical (whitespace-normalized) or — opt-in — Jaccard-
//! similar contents. Useful for spotting repeated Tailwind class strings,
//! SQL fragments, prompts, URLs, etc.

use rayon::prelude::*;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::scan_common::{self, display_path, Event, Lang, Lexer, ScanArgs, Syntax};

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

#[derive(Debug, Clone)]
struct StringLit {
    content: String, // whitespace-normalized
    line: u32,
}

struct FileStrings {
    path: PathBuf,
    strings: Vec<StringLit>,
    ext: String,
    lang: Lang,
}

// string-dup's options. A plain comment, not a doc comment: the subcommand's about text
// comes from the Commands variant in main.rs.
#[derive(clap::Args, Debug)]
#[command(args_override_self = true)]
pub(crate) struct StringDupArgs {
    #[command(flatten)]
    pub(crate) scan: ScanArgs,
    /// Shortest string to report, in characters after whitespace is collapsed
    #[arg(long, value_name = "N", default_value_t = DEFAULT_MIN_CHARS)]
    pub(crate) min_chars: usize,
    /// Fewest whitespace-separated words a reported string has
    #[arg(long, value_name = "N", default_value_t = DEFAULT_MIN_WORDS)]
    pub(crate) min_words: usize,
    /// Fewest occurrences a reported string or similar group has
    #[arg(long, value_name = "N", default_value_t = DEFAULT_MIN_OCCURRENCES)]
    pub(crate) min_occurrences: usize,
    /// Number of clusters to output, for exact and for similar clusters each
    #[arg(long, value_name = "N", default_value_t = DEFAULT_TOP)]
    pub(crate) top: usize,
    /// Also group similar, not only identical, strings by this method
    #[arg(long, value_name = "METHOD", value_parser = ["jaccard"])]
    pub(crate) similarity: Option<String>,
    /// Lowest word-set Jaccard similarity that joins two strings under --similarity jaccard
    #[arg(long, value_name = "X", default_value_t = DEFAULT_JACCARD_THRESHOLD)]
    pub(crate) threshold: f64,
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

/// The string literals of `src` (Rust char literals excluded), in source order.
/// `min_chars` only drops literals that cannot reach it before they are normalized;
/// callers still apply the exact filter.
fn extract_strings(src: &str, syntax: Syntax, min_chars: usize) -> Vec<StringLit> {
    let mut out = Vec::new();
    for ev in Lexer::new(src, syntax) {
        if let Event::Str { char_lit: false, line, content_start, content_end } = ev {
            push_string(&mut out, &src[content_start..content_end], line, min_chars);
        }
    }
    out
}

fn push_string(out: &mut Vec<StringLit>, raw: &str, line: u32, min_chars: usize) {
    // Normalizing only removes characters, and a str has at least as many bytes as
    // chars, so a raw literal shorter than min_chars bytes can never pass the filter.
    if raw.len() < min_chars {
        return;
    }
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
    // Keyed by borrowed content, so only one String per distinct content is allocated.
    let mut by_content: HashMap<&str, Vec<Occurrence>> = HashMap::new();
    for (fi, f) in files.iter().enumerate() {
        for s in &f.strings {
            by_content
                .entry(s.content.as_str())
                .or_default()
                .push(Occurrence { file_idx: fi, line: s.line });
        }
    }
    let mut variants: Vec<Variant> = by_content
        .into_iter()
        .map(|(content, occurrences)| {
            let char_count = content.chars().count();
            let word_count = content.split_ascii_whitespace().count();
            Variant { content: content.to_string(), char_count, word_count, occurrences }
        })
        .collect();
    // Content order, not HashMap order, so variant indexes (and the similar clusters
    // built from them) are the same on every run.
    variants.sort_unstable_by(|a, b| a.content.cmp(&b.content));
    variants
}

// ============================================================================
// Jaccard similarity clustering (opt-in)
// ============================================================================

/// The distinct ASCII-whitespace-separated words of each content, as sorted word ids,
/// and the number of distinct words. Ids follow first appearance, not word order;
/// only set membership matters to the Jaccard index.
fn word_id_sets(contents: &[&str]) -> (Vec<Vec<u32>>, usize) {
    let mut ids: HashMap<&str, u32> = HashMap::new();
    let sets = contents
        .iter()
        .map(|c| {
            let mut set: Vec<u32> = c
                .split_ascii_whitespace()
                .map(|w| {
                    let next = u32::try_from(ids.len()).expect("more than u32::MAX distinct words");
                    *ids.entry(w).or_insert(next)
                })
                .collect();
            set.sort_unstable();
            set.dedup();
            set
        })
        .collect();
    (sets, ids.len())
}

/// Number of ids two sorted, deduplicated id lists share.
fn intersection_len(a: &[u32], b: &[u32]) -> usize {
    let (mut i, mut j, mut n) = (0, 0, 0);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                n += 1;
                i += 1;
                j += 1;
            }
        }
    }
    n
}

/// Jaccard index of two sorted, deduplicated id lists.
fn jaccard(a: &[u32], b: &[u32]) -> f64 {
    let inter = intersection_len(a, b);
    let union = a.len() + b.len() - inter;
    if union == 0 { 0.0 } else { inter as f64 / union as f64 }
}

/// Union-find for clustering Jaccard-similar variants. `find` is iterative with path
/// halving and `union` links the smaller tree under the larger, so long chains of
/// unions neither recurse nor build deep trees.
struct UnionFind {
    parent: Vec<usize>,
    size: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self { Self { parent: (0..n).collect(), size: vec![1; n] } }
    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            let grandparent = self.parent[self.parent[x]];
            self.parent[x] = grandparent;
            x = grandparent;
        }
        x
    }
    fn union(&mut self, a: usize, b: usize) {
        let mut ra = self.find(a);
        let mut rb = self.find(b);
        if ra == rb { return; }
        if self.size[ra] < self.size[rb] { std::mem::swap(&mut ra, &mut rb); }
        self.parent[rb] = ra;
        self.size[ra] += self.size[rb];
    }
}

fn build_similar_clusters(variants: &[Variant], threshold: f64) -> Vec<Vec<usize>> {
    let n = variants.len();
    if n < 2 { return Vec::new(); }
    let contents: Vec<&str> = variants.iter().map(|v| v.content.as_str()).collect();
    let (word_sets, word_count) = word_id_sets(&contents);

    // Inverted index: word id -> variant indexes, ascending since variants are visited in order.
    let mut inverted: Vec<Vec<usize>> = vec![Vec::new(); word_count];
    for (idx, ws) in word_sets.iter().enumerate() {
        for &w in ws {
            inverted[w as usize].push(idx);
        }
    }

    let mut uf = UnionFind::new(n);

    // For each variant, compute candidates (those sharing >= 1 word).
    // Process in parallel and collect edges to union sequentially. `seen[j] == i` marks
    // j as already scored for i; each worker keeps one such array for all its i.
    let edges: Vec<(usize, usize)> = (0..n)
        .into_par_iter()
        .map_init(
            || vec![usize::MAX; n],
            |seen, i| {
                let mut local_edges: Vec<(usize, usize)> = Vec::new();
                let a = &word_sets[i];
                for &w in a {
                    let list = &inverted[w as usize];
                    for &j in &list[list.partition_point(|&j| j <= i)..] {
                        if seen[j] == i {
                            continue;
                        }
                        seen[j] = i;
                        let b = &word_sets[j];
                        // J(a, b) <= min(|a|, |b|) / max(|a|, |b|), and float division is
                        // monotone, so a pair whose size ratio is below the threshold
                        // cannot reach it.
                        let (lo, hi) = if a.len() < b.len() { (a.len(), b.len()) } else { (b.len(), a.len()) };
                        if (lo as f64 / hi as f64) < threshold {
                            continue;
                        }
                        if jaccard(a, b) >= threshold {
                            local_edges.push((i, j));
                        }
                    }
                }
                local_edges
            },
        )
        .flatten()
        .collect();

    for (a, b) in edges {
        uf.union(a, b);
    }

    // Group by root
    let mut by_root: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        let r = uf.find(i);
        by_root.entry(r).or_default().push(i);
    }
    // Members are pushed in index order; order the groups by their first member so the
    // result does not follow HashMap order.
    let mut groups: Vec<Vec<usize>> = by_root.into_values().filter(|g| g.len() >= 2).collect();
    groups.sort_unstable_by_key(|g| g[0]);
    groups
}

// ============================================================================
// Output
// ============================================================================

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

pub fn run(parsed: StringDupArgs) {
    let root = parsed.scan.root();
    let paths = parsed.scan.files(&root, DEFAULT_EXTS);

    let files: Vec<FileStrings> = paths
        .par_iter()
        .filter_map(|p| {
            let src = fs::read_to_string(p).ok()?;
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_string();
            let syntax = Syntax::for_ext(&ext);
            let lang = syntax.lang;
            let mut strings = extract_strings(&src, syntax, parsed.min_chars);
            // filter by min chars / words right at extraction
            strings.retain(|s| {
                s.content.chars().count() >= parsed.min_chars
                    && s.content.split_ascii_whitespace().count() >= parsed.min_words
            });
            Some(FileStrings { path: p.clone(), strings, ext, lang })
        })
        .collect();

    let extracted_strings: usize = files.iter().map(|f| f.strings.len()).sum();
    let language_breakdown = scan_common::language_breakdown(
        files.iter().map(|f| (f.ext.as_str(), f.lang, f.strings.len() as u64)),
        "lang",
        "strings",
    );
    let fallback_warnings = scan_common::fallback_warnings(files.iter().map(|f| (f.ext.as_str(), f.lang)));

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
            .then_with(|| a.content.cmp(&b.content))
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
        "exact_clusters": shown_exact.iter().map(|v| exact_cluster_to_json(v, &files, &root)).collect::<Vec<_>>(),
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
        // Sort: total occurrences desc, then variant count desc, then first member's content
        // (groups are disjoint, so first members differ and the order is total).
        groups_filtered.sort_by(|a, b| {
            let total_a: usize = a.iter().map(|&i| variants[i].occurrences.len()).sum();
            let total_b: usize = b.iter().map(|&i| variants[i].occurrences.len()).sum();
            total_b
                .cmp(&total_a)
                .then_with(|| b.len().cmp(&a.len()))
                .then_with(|| variants[a[0]].content.cmp(&variants[b[0]].content))
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
                    .map(|g| similar_cluster_to_json(g, &variants, &files, &root))
                    .collect(),
            ),
        );
    }

    println!("{}", serde_json::to_string_pretty(&result).unwrap());
}

#[cfg(test)]
mod rust_quote_tests {
    use super::*;

    const FIXTURE: &str = include_str!("testdata/rust_quotes.txt");

    fn contents(src: &str, rust: bool) -> Vec<String> {
        extract_strings(src, Syntax { lang: Lang::CFamily, rust }, 0).into_iter().map(|s| s.content).collect()
    }

    #[test]
    fn rust_quote_string_dup_extracts_only_string_literals() {
        // Lifetimes and char literals are skipped; the raw string keeps its inner quotes.
        assert_eq!(contents(FIXTURE, true), vec!["a \"q\" b".to_string(), "lit".to_string()]);
    }

    #[test]
    fn rust_quote_string_dup_raw_string_line() {
        let got: Vec<(String, u32)> =
            extract_strings("r#\"x \"y\"\nz\"#\n\"c\"", Syntax { lang: Lang::CFamily, rust: true }, 0)
                .into_iter()
                .map(|s| (s.content, s.line))
                .collect();
        assert_eq!(got, vec![("x \"y\" z".to_string(), 1), ("c".to_string(), 3)]);
    }

    #[test]
    fn rust_quote_rules_do_not_apply_to_ts() {
        assert_eq!(
            contents("x = 'it\\'s'; f<'y>; z = \"w\"", false),
            vec!["it\\'s".to_string(), "y>; z = \"w\"".to_string()]
        );
    }
}

#[cfg(test)]
mod similarity_tests {
    use super::*;
    use std::collections::{BTreeMap, HashSet};

    /// The String/HashSet Jaccard index the id-based one replaced.
    fn jaccard_by_words(a: &str, b: &str) -> f64 {
        let sa: HashSet<&str> = a.split_ascii_whitespace().collect();
        let sb: HashSet<&str> = b.split_ascii_whitespace().collect();
        let inter = sa.intersection(&sb).count();
        let union = sa.union(&sb).count();
        if union == 0 { 0.0 } else { inter as f64 / union as f64 }
    }

    /// Deterministic contents drawn from a small vocabulary, so pairs share words often
    /// and repeat words within one content.
    fn corpus(count: usize, seed: u64) -> Vec<String> {
        const VOCAB: &[&str] = &[
            "flex", "items-center", "gap-2", "p-4", "text-sm", "font-bold", "rounded",
            "bg-white", "dark:bg-black", "w-full", "h-8", "border", "mt-1", "ü", "日本",
        ];
        let mut state = seed;
        let mut next = move |m: usize| {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((state >> 33) as usize) % m
        };
        (0..count)
            .map(|_| {
                let words = next(9);
                (0..words).map(|_| VOCAB[next(VOCAB.len())]).collect::<Vec<_>>().join(" ")
            })
            .collect()
    }

    fn variants_of(contents: &[String]) -> Vec<Variant> {
        let mut uniq: Vec<String> = contents.to_vec();
        uniq.sort();
        uniq.dedup();
        uniq.into_iter()
            .map(|content| Variant {
                char_count: content.chars().count(),
                word_count: content.split_ascii_whitespace().count(),
                content,
                occurrences: vec![Occurrence { file_idx: 0, line: 1 }],
            })
            .collect()
    }

    #[test]
    fn jaccard_matches_word_set_jaccard() {
        let contents = corpus(300, 7);
        let refs: Vec<&str> = contents.iter().map(|s| s.as_str()).collect();
        let (sets, _) = word_id_sets(&refs);
        for i in 0..refs.len() {
            for j in 0..refs.len() {
                assert_eq!(
                    jaccard(&sets[i], &sets[j]).to_bits(),
                    jaccard_by_words(refs[i], refs[j]).to_bits(),
                    "{:?} vs {:?}",
                    refs[i],
                    refs[j]
                );
            }
        }
    }

    #[test]
    fn similar_clusters_match_all_pairs_scan() {
        let variants = variants_of(&corpus(400, 11));
        for threshold in [0.0, 0.2, 0.34, 0.5, 2.0 / 3.0, 0.7, 0.75, 1.0] {
            let mut uf = UnionFind::new(variants.len());
            for i in 0..variants.len() {
                for j in i + 1..variants.len() {
                    let (a, b) = (&variants[i].content, &variants[j].content);
                    let shares_word = a.split_ascii_whitespace().any(|w| b.split_ascii_whitespace().any(|x| x == w));
                    if shares_word && jaccard_by_words(a, b) >= threshold {
                        uf.union(i, j);
                    }
                }
            }
            let mut by_root: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
            for i in 0..variants.len() {
                by_root.entry(uf.find(i)).or_default().push(i);
            }
            let mut want: Vec<Vec<usize>> = by_root.into_values().filter(|g| g.len() >= 2).collect();
            want.sort_unstable_by_key(|g| g[0]);
            assert_eq!(build_similar_clusters(&variants, threshold), want, "threshold {threshold}");
        }
    }

    #[test]
    fn union_find_handles_long_chains() {
        const N: usize = 100_000;
        let mut uf = UnionFind::new(N);
        for i in 0..N - 1 {
            uf.union(i, i + 1);
        }
        for i in (0..N - 1).rev() {
            uf.union(i + 1, i);
        }
        let root = uf.find(0);
        assert!((0..N).all(|i| uf.find(i) == root));

        // A parent chain built directly, as a union without size balancing would leave it.
        let mut chain = UnionFind::new(N);
        for i in 0..N - 1 {
            chain.parent[i] = i + 1;
        }
        assert_eq!(chain.find(0), N - 1);
        assert!((0..N).all(|i| chain.find(i) == N - 1));
    }

    #[test]
    fn short_raw_literals_are_dropped_before_normalizing() {
        let src = "a = \"ab  cd\"; b = \"\u{3000}xy\u{3000}\"; c = \"abcdef\"";
        let got: Vec<String> =
            extract_strings(src, Syntax::for_ext("ts"), 6).into_iter().map(|s| s.content).collect();
        // "ab  cd" has 6 bytes and is kept here; run() drops it after normalizing (5 chars).
        // "\u{3000}xy\u{3000}" has 8 bytes and is kept; its normalized "xy" is short too.
        assert_eq!(got, vec!["ab cd".to_string(), "xy".to_string(), "abcdef".to_string()]);
        let none: Vec<String> =
            extract_strings(src, Syntax::for_ext("ts"), 9).into_iter().map(|s| s.content).collect();
        assert!(none.is_empty());
    }
}
