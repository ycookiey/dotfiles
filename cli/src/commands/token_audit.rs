//! Token consumption audit. Replaces bin/token-audit (bash + jq).
//! Direct std::fs usage (no process spawn) for performance on Windows.

use memchr::memmem;
use rayon::prelude::*;
use serde::de::{Deserialize, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::fs;
use std::io::{BufRead, BufReader};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Assistant message extracted from JSONL line. Equivalent to bash JQ_DEDUP.
#[derive(Debug, Clone, PartialEq)]
struct Request {
    req_id: String,
    tools: Vec<ToolUse>,
    input: i64,
    output: i64,
    cache_read: i64,
    cache_create: i64,
    /// total_ctx growth from the previous request of the same file
    /// (populated by parse_file).
    delta_cc: i64,
    /// Index of the file the requestId was first seen in.
    file_idx: usize,
}

/// One tool_use block, keeping only the input fields the reports read
/// (the rest of .input, e.g. Write content, is never copied).
#[derive(Debug, Clone, PartialEq)]
struct ToolUse {
    /// tool_use block id; None when the log omits it (never deduplicated).
    id: Option<String>,
    name: String,
    /// .input.file_path (Read paths, large-op labels).
    file_path: StrField,
    /// .input.query when it is a string (ToolSearch).
    query: Option<String>,
}

impl Request {
    fn total_ctx(&self) -> i64 {
        self.input + self.cache_read + self.cache_create
    }

    /// Fold a later log line of the same request into this one.
    /// Claude Code writes each content block (thinking/text/tool_use) of one
    /// API request on its own line under the same requestId, so tools are
    /// appended (a tool_use id already present is skipped) and usage is taken
    /// from the later line.
    fn merge_line(&mut self, later: Request) {
        for t in later.tools {
            let dup = t
                .id
                .as_deref()
                .map_or(false, |id| self.tools.iter().any(|u| u.id.as_deref() == Some(id)));
            if !dup {
                self.tools.push(t);
            }
        }
        self.input = later.input;
        self.output = later.output;
        self.cache_read = later.cache_read;
        self.cache_create = later.cache_create;
    }

    /// Tool combination key: ["Edit", "Read"] -> "Edit+Read"
    /// No tools -> "(response)"
    fn tool_key(&self) -> String {
        if self.tools.is_empty() {
            "(response)".to_string()
        } else {
            let mut names: Vec<&str> = self.tools.iter().map(|t| t.name.as_str()).collect();
            names.sort();
            names.dedup();
            names.join("+")
        }
    }
}

// ============================================================================
// File discovery
// ============================================================================

/// Normalize path to use forward slashes consistently.
fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// A session log found during discovery, with the mtime read while listing
/// its directory so `--last N` needs no second stat per file.
#[derive(Debug, Clone)]
struct JsonlFile {
    path: PathBuf,
    /// None when the file's time could not be read; such files are skipped
    /// by `--last N` and kept by every other mode.
    mtime: Option<SystemTime>,
}

/// Find all *.jsonl files under ~/.claude/projects and ~/.claude-*/projects.
/// Includes subagents/ directory.
///
/// Several ~/.claude-* profiles may share one projects tree through a symlink,
/// so projects directories that resolve to the same place are walked once.
/// Files are then deduplicated by basename (first wins, ~/.claude first) and
/// returned sorted by path so every run yields the same order.
fn find_jsonl_files() -> Vec<JsonlFile> {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return Vec::new(),
    };

    let mut claude_dirs: Vec<PathBuf> = Vec::new();
    if home.join(".claude").exists() {
        claude_dirs.push(home.join(".claude"));
    }

    // Find .claude-* directories (name check first: is_dir stats the entry)
    if let Ok(entries) = fs::read_dir(&home) {
        let mut extra: Vec<PathBuf> = entries
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(".claude-"))
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        // read_dir order is not guaranteed; fix it so "first wins" is stable.
        extra.sort();
        claude_dirs.extend(extra);
    }

    discover_jsonl(&claude_dirs)
}

/// Collect `<dir>/projects/**/*.jsonl` for each profile dir in order; see
/// `find_jsonl_files` for the dedup and ordering rules.
fn discover_jsonl(claude_dirs: &[PathBuf]) -> Vec<JsonlFile> {
    let files = walk_projects_dirs(claude_dirs);

    // Deduplicate by basename (first wins)
    let mut seen: HashSet<String> = HashSet::new();
    let mut unique: Vec<JsonlFile> = Vec::with_capacity(files.len());
    for f in files {
        if let Some(name) = f.path.file_name() {
            if seen.insert(name.to_string_lossy().into_owned()) {
                unique.push(f);
            }
        }
    }

    unique.sort_by(|a, b| a.path.cmp(&b.path));
    unique
}

/// Walk each `<dir>/projects` once: dirs that resolve to the same place
/// (a profile whose projects is a symlink to another's) are skipped.
fn walk_projects_dirs(claude_dirs: &[PathBuf]) -> Vec<JsonlFile> {
    let mut walked: HashSet<PathBuf> = HashSet::new();
    let mut files: Vec<JsonlFile> = Vec::new();
    for claude_dir in claude_dirs {
        let projects_dir = claude_dir.join("projects");
        if !projects_dir.exists() {
            continue;
        }
        // The canonical form (\\?\ on Windows) is only a key; paths handed out
        // keep the original spelling. If it cannot be resolved, the original
        // path is the key and the dir is walked as before.
        let key = fs::canonicalize(&projects_dir).unwrap_or_else(|_| projects_dir.clone());
        if walked.insert(key) {
            collect_jsonl(&projects_dir, &mut files);
        }
    }
    files
}

/// Recursively collect *.jsonl files with their mtimes.
/// Uses the entry's own file type and metadata (taken from the directory
/// listing on Windows); only symlinks are followed with an extra stat.
fn collect_jsonl(dir: &Path, out: &mut Vec<JsonlFile>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        let path = entry.path();
        // Symlinks are followed; a broken one counts as a file without an
        // mtime, as the former path.is_dir() + fs::metadata pair treated it.
        let followed = file_type.is_symlink().then(|| fs::metadata(&path).ok());
        let is_dir = match &followed {
            Some(m) => m.as_ref().map_or(false, |m| m.is_dir()),
            None => file_type.is_dir(),
        };
        if is_dir {
            collect_jsonl(&path, out);
        } else if path.extension().map_or(false, |e| e == "jsonl") {
            let mtime = match followed {
                Some(m) => m.and_then(|m| m.modified().ok()),
                None => entry.metadata().and_then(|m| m.modified()).ok(),
            };
            out.push(JsonlFile { path, mtime });
        }
    }
}

/// --last N: sort by mtime descending, take top N.
/// Equal mtimes keep the input (path) order, so the pick is deterministic.
fn find_latest_files(files: &[JsonlFile], n: usize) -> Vec<JsonlFile> {
    let mut with_mtime: Vec<(SystemTime, &JsonlFile)> = files
        .iter()
        .filter_map(|f| f.mtime.map(|t| (t, f)))
        .collect();

    with_mtime.sort_by(|a, b| b.0.cmp(&a.0)); // descending, stable
    with_mtime.truncate(n);
    with_mtime.into_iter().map(|(_, f)| f.clone()).collect()
}

/// --session ID: filter files containing ID in filename.
fn find_session_files(files: &[JsonlFile], id: &str) -> Vec<JsonlFile> {
    files
        .iter()
        .filter(|f| {
            f.path
                .file_name()
                .and_then(|n| n.to_str())
                .map_or(false, |s| s.contains(id))
        })
        .cloned()
        .collect()
}

/// Encode a project path into the project directory names Claude Code gives it
/// (both conventions), e.g.
/// C:\Main\Project\Foo -> ["C--Main-Project-Foo", "C--Main--Project--Foo"],
/// C:\Main\Project\Foo\.claude\worktrees\b4 -> ["C--Main-Project-Foo--claude-worktrees-b4", ...].
/// Accepts C:\..., C:/... and the Git Bash form /c/...; a drive alone (C:, C:\, /c)
/// is the drive root, "C--". Any other input yields no names (no filtering).
fn encode_project_prefixes(project_path: &str) -> Vec<String> {
    let normalized = project_path.replace('\\', "/");
    let mut chars = normalized.chars();
    // Drive letter and the rest, without byte slicing (the input may be any UTF-8).
    let (drive, rest) = match (chars.next(), chars.next(), chars.next()) {
        // Windows-style: C:/Main/Project/Foo, C:
        (Some(d), Some(':'), next) if d.is_ascii_alphabetic() && matches!(next, None | Some('/')) => {
            (d, &normalized[2..])
        }
        // Git Bash style: /c/Main/Project/Foo, /c
        (Some('/'), Some(d), next) if d.is_ascii_alphabetic() && matches!(next, None | Some('/')) => {
            (d, &normalized[2..])
        }
        _ => return Vec::new(),
    };
    let drive = drive.to_ascii_uppercase();
    let components: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();

    // Current convention: the whole path with every char but [A-Za-z0-9] turned into '-'
    // (C:\Main\.claude -> C--Main--claude).
    let new_conv = encode_dir_name(&format!("{}:/{}", drive, components.join("/")));
    // Old convention: C--Main--Project--Foo (double dash between components)
    let old_conv = format!(
        "{}--{}",
        drive,
        components.iter().map(|c| encode_dir_name(c)).collect::<Vec<_>>().join("--")
    );

    if new_conv == old_conv {
        vec![new_conv]
    } else {
        vec![new_conv, old_conv]
    }
}

/// Claude Code's project dir encoding: each char outside [A-Za-z0-9] becomes '-'.
/// TODO: assumed from observed names (all ASCII and under 120 chars): a non-ASCII char
/// is taken as one '-' per UTF-16 unit (a JS regex replace), and Claude Code's handling
/// of very long names (truncation) is not reproduced.
fn encode_dir_name(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for c in path.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else {
            for _ in 0..c.len_utf16() {
                out.push('-');
            }
        }
    }
    out
}

/// Whether a project dir name belongs to the project encoded as `enc`: the project
/// itself, or one of its worktrees (enc--claude-worktrees-*). Sessions started in a
/// subdirectory (enc-sub) are not included: the encoding cannot tell them from a
/// sibling project (C:\Main\ch-data vs C:\Main\ch-data-model).
fn project_dir_matches(name: &str, enc: &str) -> bool {
    name == enc
        || name
            .strip_prefix(enc)
            .is_some_and(|rest| rest.starts_with("--claude-worktrees-"))
}

/// Filter JSONL files to those belonging to a project (see project_dir_matches).
fn filter_by_project(files: &[JsonlFile], project_path: &str) -> Vec<JsonlFile> {
    let prefixes = encode_project_prefixes(project_path);
    if prefixes.is_empty() {
        return files.to_vec();
    }

    files
        .iter()
        .filter(|f| {
            // Walk up to find the project directory (parent of the JSONL or subagents/ dir)
            for ancestor in f.path.ancestors().skip(1) {
                if let Some(name) = ancestor.file_name().and_then(|n| n.to_str()) {
                    if prefixes.iter().any(|enc| project_dir_matches(name, enc)) {
                        return true;
                    }
                }
            }
            false
        })
        .cloned()
        .collect()
}

// ============================================================================
// JSONL parsing
// ============================================================================

/// Requests of a set of session logs, with each file's context stats.
struct ParsedLogs {
    /// One Request per requestId (see parse_logs).
    requests: Vec<Request>,
    /// Stats of each file that has requests, in file order.
    sessions: Vec<SessionCtx>,
}

/// total_ctx stats of one file's own requests: after its lines are merged
/// and before the cross-file dedup, as if the file were parsed alone.
#[derive(Debug, Clone, Copy, PartialEq)]
struct SessionCtx {
    file_idx: usize,
    first_total_ctx: i64,
    max_ctx: i64,
}

/// One file's requests (delta_cc set) and its stats (None without requests).
struct ParsedFile {
    requests: Vec<Request>,
    ctx: Option<SessionCtx>,
}

/// Parse JSONL files into one Request per requestId, with delta_cc set.
/// Each file is parsed on its own (see parse_file) and its deltas are taken
/// over that file's own request sequence. A requestId already seen in an
/// earlier file (a forked or resumed session repeats its history) is skipped:
/// the first file's version and position win.
fn parse_logs(files: &[PathBuf]) -> ParsedLogs {
    // Files are parsed in parallel; collect keeps file order, so the merge
    // below sees them in the same order as a sequential parse.
    let parsed: Vec<ParsedFile> = files
        .par_iter()
        .enumerate()
        .map(|(file_idx, file_path)| parse_file(file_path, file_idx))
        .collect();

    let mut requests: Vec<Request> = Vec::new();
    let mut sessions: Vec<SessionCtx> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for file in parsed {
        sessions.extend(file.ctx);
        for req in file.requests {
            if !seen.contains(&req.req_id) {
                seen.insert(req.req_id.clone());
                requests.push(req);
            }
        }
    }

    ParsedLogs { requests, sessions }
}

/// Every assistant line carries this key; lines without it are not parsed.
const REQUEST_ID_KEY: &[u8] = b"\"requestId\"";

/// Parse one session log. Lines sharing a requestId are merged into one
/// Request at the position of its first line (see Request::merge_line), then
/// delta_cc is computed over the file's requests in that order.
fn parse_file(file_path: &Path, file_idx: usize) -> ParsedFile {
    let mut requests: Vec<Request> = Vec::new();
    let mut id_index: HashMap<String, usize> = HashMap::new();

    if let Ok(f) = fs::File::open(file_path) {
        let mut reader = BufReader::with_capacity(1 << 16, f);
        let mut line: Vec<u8> = Vec::new();
        let mut buffers = simd_json::Buffers::default();
        let has_request_id = memmem::Finder::new(REQUEST_ID_KEY);
        loop {
            line.clear();
            match reader.read_until(b'\n', &mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            // A Request needs a string requestId, so a line without the key
            // (user turns, tool results, metadata) is skipped unparsed. The
            // logs are written by JSON.stringify, which never escapes the
            // ASCII letters of a key.
            if has_request_id.find(&line).is_none() {
                continue;
            }
            let mut req = match parse_line(&mut line, &mut buffers) {
                Some(r) => r,
                None => continue,
            };
            req.file_idx = file_idx;
            match id_index.get(&req.req_id) {
                Some(&idx) => requests[idx].merge_line(req),
                None => {
                    id_index.insert(req.req_id.clone(), requests.len());
                    requests.push(req);
                }
            }
        }
    }

    compute_deltas(&mut requests);
    let ctx = requests.first().map(|first| SessionCtx {
        file_idx,
        first_total_ctx: first.total_ctx(),
        max_ctx: requests.iter().map(Request::total_ctx).max().unwrap_or(0),
    });
    ParsedFile { requests, ctx }
}

/// Request of one log line, or None when the line is not valid JSON or not
/// an assistant message: `type` or `message.role` is "assistant",
/// `message.usage` is an object and `requestId` is a string. The buffer is
/// rewritten by the parser.
fn parse_line(line: &mut [u8], buffers: &mut simd_json::Buffers) -> Option<Request> {
    let line: Obj<LogLine> = simd_json::serde::from_slice_with_buffers(line, buffers).ok()?;
    let line = line.0?;
    let message = line.message.0?;
    let is_assistant = line.typ.as_str() == Some("assistant")
        || message.role.as_str() == Some("assistant");
    let usage = message.usage.0?;
    let req_id = line.request_id.into_string()?;
    if !is_assistant {
        return None;
    }

    let tools = message
        .content
        .0
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| {
            let item = item.0?;
            if item.typ.as_str() != Some("tool_use") {
                return None;
            }
            let name = item.name.into_string()?;
            let input = item.input.0.unwrap_or_default();
            Some(ToolUse {
                id: item.id.into_string(),
                name,
                file_path: input.file_path,
                query: input.query.into_string(),
            })
        })
        .collect();

    Some(Request {
        req_id,
        tools,
        input: usage.input_tokens.0.unwrap_or(0),
        output: usage.output_tokens.0.unwrap_or(0),
        cache_read: usage.cache_read_input_tokens.0.unwrap_or(0),
        cache_create: usage.cache_creation_input_tokens.0.unwrap_or(0),
        delta_cc: 0, // populated later by compute_deltas()
        file_idx: 0, // populated later by parse_file()
    })
}

// ----------------------------------------------------------------------------
// Typed log line
//
// Only the fields the reports use are read; everything else is skipped
// without being copied. A field of an unexpected JSON type never fails the
// line: it reads as absent, the way serde_json::Value's accessors
// (`v["x"].as_str()`, `.as_i64()`, `.as_array()`, `.is_object()`) see it,
// so the result matches a parse into Value. A repeated key keeps its last
// value, as Value does. #[serde(untagged)] is avoided: it would buffer whole
// subtrees (Write content and the like) to try each variant.
// ----------------------------------------------------------------------------

/// A JSON value read leniently: the kinds the type accepts are converted,
/// any other kind becomes `other()`.
trait Loose: Sized {
    fn other() -> Self;
    fn from_str(_: &str) -> Self {
        Self::other()
    }
    fn from_i64(_: i64) -> Self {
        Self::other()
    }
    fn from_u64(_: u64) -> Self {
        Self::other()
    }
    fn from_seq<'de, A: SeqAccess<'de>>(mut seq: A) -> Result<Self, A::Error> {
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(Self::other())
    }
    fn from_map<'de, A: MapAccess<'de>>(mut map: A) -> Result<Self, A::Error> {
        while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
        Ok(Self::other())
    }
}

struct LooseVisitor<T>(PhantomData<T>);

impl<'de, T: Loose> Visitor<'de> for LooseVisitor<T> {
    type Value = T;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("any JSON value")
    }
    fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<T, E> {
        Ok(T::other())
    }
    fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<T, E> {
        Ok(T::from_i64(v))
    }
    fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<T, E> {
        Ok(T::from_u64(v))
    }
    fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<T, E> {
        Ok(T::other())
    }
    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<T, E> {
        Ok(T::from_str(v))
    }
    fn visit_unit<E: serde::de::Error>(self) -> Result<T, E> {
        Ok(T::other())
    }
    fn visit_none<E: serde::de::Error>(self) -> Result<T, E> {
        Ok(T::other())
    }
    fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<T, D::Error> {
        d.deserialize_any(self)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<T, A::Error> {
        T::from_seq(seq)
    }
    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<T, A::Error> {
        T::from_map(map)
    }
}

fn deserialize_loose<'de, D: Deserializer<'de>, T: Loose>(d: D) -> Result<T, D::Error> {
    d.deserialize_any(LooseVisitor(PhantomData))
}

/// A string field as Value sees it. NotStr is kept apart from Missing
/// because the large-op labels print a present non-string value as "".
#[derive(Debug, Clone, Default, PartialEq)]
enum StrField {
    #[default]
    Missing,
    Str(String),
    NotStr,
}

impl StrField {
    /// `v["x"].as_str()`
    fn as_str(&self) -> Option<&str> {
        match self {
            StrField::Str(s) => Some(s),
            _ => None,
        }
    }

    fn into_string(self) -> Option<String> {
        match self {
            StrField::Str(s) => Some(s),
            _ => None,
        }
    }

    /// `v.get("x").map(|x| x.as_str().unwrap_or(""))`
    fn as_label(&self) -> Option<&str> {
        match self {
            StrField::Missing => None,
            StrField::Str(s) => Some(s),
            StrField::NotStr => Some(""),
        }
    }
}

impl Loose for StrField {
    fn other() -> Self {
        StrField::NotStr
    }
    fn from_str(v: &str) -> Self {
        StrField::Str(v.to_owned())
    }
}

impl<'de> Deserialize<'de> for StrField {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        deserialize_loose(d)
    }
}

/// An integer field: `v["x"].as_i64()` (None for floats, strings and
/// integers beyond i64).
#[derive(Debug, Default)]
struct IntField(Option<i64>);

impl Loose for IntField {
    fn other() -> Self {
        IntField(None)
    }
    fn from_i64(v: i64) -> Self {
        IntField(Some(v))
    }
    fn from_u64(v: u64) -> Self {
        IntField(i64::try_from(v).ok())
    }
}

impl<'de> Deserialize<'de> for IntField {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        deserialize_loose(d)
    }
}

/// The fields an object type reads; `KEYS[i]` is passed to `read` as `i`.
trait Fields: Default {
    const KEYS: &'static [&'static str];
    fn read<'de, A: MapAccess<'de>>(&mut self, key: usize, map: &mut A) -> Result<(), A::Error>;
}

/// An object read into T, or None when the value is not an object.
#[derive(Debug)]
struct Obj<T>(Option<T>);

impl<T> Default for Obj<T> {
    fn default() -> Self {
        Obj(None)
    }
}

impl<T: Fields> Loose for Obj<T> {
    fn other() -> Self {
        Obj(None)
    }
    fn from_map<'de, A: MapAccess<'de>>(mut map: A) -> Result<Self, A::Error> {
        let mut out = T::default();
        while let Some(key) = map.next_key_seed(KeyIndex(T::KEYS))? {
            match key {
                Some(i) => out.read(i, &mut map)?,
                None => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(Obj(Some(out)))
    }
}

impl<'de, T: Fields> Deserialize<'de> for Obj<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        deserialize_loose(d)
    }
}

/// An array of T, or None when the value is not an array.
#[derive(Debug)]
struct Arr<T>(Option<Vec<T>>);

impl<T> Default for Arr<T> {
    fn default() -> Self {
        Arr(None)
    }
}

impl<T: for<'de> Deserialize<'de>> Loose for Arr<T> {
    fn other() -> Self {
        Arr(None)
    }
    fn from_seq<'de, A: SeqAccess<'de>>(mut seq: A) -> Result<Self, A::Error> {
        let mut out = Vec::new();
        while let Some(item) = seq.next_element::<T>()? {
            out.push(item);
        }
        Ok(Arr(Some(out)))
    }
}

impl<'de, T: for<'a> Deserialize<'a>> Deserialize<'de> for Arr<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        deserialize_loose(d)
    }
}

/// Map key seed: the key's index in a field list, None for other keys.
struct KeyIndex(&'static [&'static str]);

impl<'de> serde::de::DeserializeSeed<'de> for KeyIndex {
    type Value = Option<usize>;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Option<usize>, D::Error> {
        d.deserialize_str(self)
    }
}

impl<'de> Visitor<'de> for KeyIndex {
    type Value = Option<usize>;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("an object key")
    }
    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Option<usize>, E> {
        Ok(self.0.iter().position(|k| *k == v))
    }
}

#[derive(Debug, Default)]
struct LogLine {
    typ: StrField,
    request_id: StrField,
    message: Obj<LogMessage>,
}

impl Fields for LogLine {
    const KEYS: &'static [&'static str] = &["type", "requestId", "message"];
    fn read<'de, A: MapAccess<'de>>(&mut self, key: usize, map: &mut A) -> Result<(), A::Error> {
        match key {
            0 => self.typ = map.next_value()?,
            1 => self.request_id = map.next_value()?,
            _ => self.message = map.next_value()?,
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
struct LogMessage {
    role: StrField,
    usage: Obj<Usage>,
    content: Arr<Obj<ContentItem>>,
}

impl Fields for LogMessage {
    const KEYS: &'static [&'static str] = &["role", "usage", "content"];
    fn read<'de, A: MapAccess<'de>>(&mut self, key: usize, map: &mut A) -> Result<(), A::Error> {
        match key {
            0 => self.role = map.next_value()?,
            1 => self.usage = map.next_value()?,
            _ => self.content = map.next_value()?,
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
struct Usage {
    input_tokens: IntField,
    output_tokens: IntField,
    cache_read_input_tokens: IntField,
    cache_creation_input_tokens: IntField,
}

impl Fields for Usage {
    const KEYS: &'static [&'static str] = &[
        "input_tokens",
        "output_tokens",
        "cache_read_input_tokens",
        "cache_creation_input_tokens",
    ];
    fn read<'de, A: MapAccess<'de>>(&mut self, key: usize, map: &mut A) -> Result<(), A::Error> {
        let v = map.next_value()?;
        match key {
            0 => self.input_tokens = v,
            1 => self.output_tokens = v,
            2 => self.cache_read_input_tokens = v,
            _ => self.cache_creation_input_tokens = v,
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
struct ContentItem {
    typ: StrField,
    id: StrField,
    name: StrField,
    input: Obj<ToolInput>,
}

impl Fields for ContentItem {
    const KEYS: &'static [&'static str] = &["type", "id", "name", "input"];
    fn read<'de, A: MapAccess<'de>>(&mut self, key: usize, map: &mut A) -> Result<(), A::Error> {
        match key {
            0 => self.typ = map.next_value()?,
            1 => self.id = map.next_value()?,
            2 => self.name = map.next_value()?,
            _ => self.input = map.next_value()?,
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
struct ToolInput {
    file_path: StrField,
    query: StrField,
}

impl Fields for ToolInput {
    const KEYS: &'static [&'static str] = &["file_path", "query"];
    fn read<'de, A: MapAccess<'de>>(&mut self, key: usize, map: &mut A) -> Result<(), A::Error> {
        match key {
            0 => self.file_path = map.next_value()?,
            _ => self.query = map.next_value()?,
        }
        Ok(())
    }
}

/// Compute delta_cc: total_ctx growth from previous request.
/// Resets at session (file) boundaries. Captures how much context each request added.
/// Run it on one file's requests before the cross-file dedup: on the merged
/// list a file boundary would charge a request's whole total_ctx as its delta.
fn compute_deltas(requests: &mut [Request]) {
    let mut prev_ctx: i64 = 0;
    let mut prev_file_idx: usize = usize::MAX;
    for r in requests.iter_mut() {
        if r.file_idx != prev_file_idx {
            prev_ctx = 0;
            prev_file_idx = r.file_idx;
        }
        let ctx = r.total_ctx();
        let delta = ctx - prev_ctx;
        r.delta_cc = delta.max(0); // clamp negative (compaction) to 0
        prev_ctx = ctx;
    }
}

// ============================================================================
// Subcommands
// ============================================================================

/// cmd_static: static token analysis of config files.
fn cmd_static() -> Value {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return json!({"error": "no home directory"}),
    };

    // CLAUDE.md analysis
    let mut claude_md = Vec::new();
    let global_claude_md = home.join(".claude/CLAUDE.md");
    if global_claude_md.exists() {
        if let Ok(bytes) = fs::metadata(&global_claude_md).map(|m| m.len()) {
            let est_tokens = (bytes / 2) as i64;
            claude_md.push(json!({
                "path": normalize_path(&global_claude_md),
                "bytes": bytes,
                "est_tokens": est_tokens
            }));
        }
    }

    // Find project CLAUDE.md
    if let Ok(cwd) = std::env::current_dir() {
        for ancestor in cwd.ancestors() {
            let claude_md_path = ancestor.join("CLAUDE.md");
            if claude_md_path.exists() {
                if let Ok(bytes) = fs::metadata(&claude_md_path).map(|m| m.len()) {
                    let est_tokens = (bytes / 2) as i64;
                    let path_str = normalize_path(&claude_md_path);
                    // Avoid duplicate
                    if !claude_md.iter().any(|x| x["path"] == path_str) {
                        claude_md.push(json!({
                            "path": path_str,
                            "bytes": bytes,
                            "est_tokens": est_tokens
                        }));
                    }
                }
                break;
            }
        }
    }

    // Skills analysis (frontmatter description)
    let mut skills = Vec::new();
    let mut skills_total_est_tokens = 0i64;
    let mut processed_claude_dir = false;

    for claude_dir_prefix in &[home.join(".claude")] {
        let skills_dir = claude_dir_prefix.join("skills");
        if processed_claude_dir {
            break;
        }
        if skills_dir.exists() {
            if let Ok(entries) = fs::read_dir(&skills_dir) {
                for entry in entries.flatten() {
                    let skill_dir = entry.path();
                    let skill_md = skill_dir.join("SKILL.md");
                    if skill_md.exists() {
                        if let Ok(content) = fs::read_to_string(&skill_md) {
                            let skill_name = skill_dir
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("unknown")
                                .to_string();

                            // Extract frontmatter (--- delimited)
                            let description_bytes = extract_frontmatter_description(&content);
                            let est_tokens = (description_bytes / 2) as i64;
                            skills_total_est_tokens += est_tokens;

                            skills.push(json!({
                                "name": skill_name,
                                "desc_bytes": description_bytes,
                                "est_tokens": est_tokens
                            }));
                        }
                    }
                }
            }
            processed_claude_dir = true;
        }
    }

    // settings.json analysis
    let settings_path = home.join(".claude/settings.json");
    let mut plugins: Vec<String> = Vec::new();
    let mut mcp_server_count = 0i64;
    let mut token_audit_hook_active = false;

    if settings_path.exists() {
        if let Ok(content) = fs::read_to_string(&settings_path) {
            if let Ok(settings) = serde_json::from_str::<Value>(&content) {
                // enabledPlugins
                if let Some(enabled) = settings.get("enabledPlugins").and_then(|x| x.as_object()) {
                    for (k, v) in enabled {
                        if v.as_bool() == Some(true) {
                            plugins.push(k.clone());
                        }
                    }
                }

                // Hooks check: settings.json has hooks.PostToolUse[].hooks[].command structure
                if let Some(hooks) = settings.get("hooks") {
                    if let Some(post_use) = hooks["PostToolUse"].as_array() {
                        for hook_config in post_use {
                            // Check nested hooks array
                            if let Some(nested_hooks) = hook_config.get("hooks").and_then(|x| x.as_array()) {
                                for hook in nested_hooks {
                                    if let Some(command) = hook.get("command").and_then(|x| x.as_str()) {
                                        if command.contains("token-audit") {
                                            token_audit_hook_active = true;
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // MCP servers count
    let claude_json_path = home.join(".claude/.claude.json");
    if claude_json_path.exists() {
        if let Ok(content) = fs::read_to_string(&claude_json_path) {
            if let Ok(claude_json) = serde_json::from_str::<Value>(&content) {
                if let Some(mcp) = claude_json.get("mcpServers").and_then(|x| x.as_object()) {
                    mcp_server_count = mcp.len() as i64;
                }
            }
        }
    }

    json!({
        "claude_md": claude_md,
        "skills": skills,
        "skills_total_est_tokens": skills_total_est_tokens,
        "plugins": plugins,
        "plugin_count": plugins.len() as i64,
        "mcp_server_count": mcp_server_count,
        "token_audit_hook_active": token_audit_hook_active
    })
}

/// Extract description bytes from frontmatter (--- delimited).
fn extract_frontmatter_description(content: &str) -> usize {
    let lines: Vec<&str> = content.lines().collect();
    let mut in_frontmatter = false;
    for line in lines {
        if line.trim() == "---" {
            in_frontmatter = !in_frontmatter;
            continue;
        }
        if in_frontmatter {
            if let Some(rest) = line.strip_prefix("description:") {
                return rest.trim().bytes().len();
            }
        }
    }
    0
}

/// Resolve files + parse + compute deltas. Returns None when no files match.
fn collect_requests(
    mode: &str,
    value: &str,
    project: Option<&str>,
) -> Option<(Vec<PathBuf>, ParsedLogs)> {
    let all_files = find_jsonl_files();
    let all_files = match project {
        Some(path) => filter_by_project(&all_files, path),
        None => all_files,
    };
    let files: Vec<PathBuf> = match mode {
        "all" => all_files,
        "last" => {
            let n = value.parse().unwrap_or(10);
            if n == 0 { all_files } else { find_latest_files(&all_files, n) }
        }
        "session" => find_session_files(&all_files, value),
        _ => {
            eprintln!("Unknown session mode: {}", mode);
            std::process::exit(1);
        }
    }
    .into_iter()
    .map(|f| f.path)
    .collect();

    if files.is_empty() {
        return None;
    }

    let parsed = parse_logs(&files);
    Some((files, parsed))
}

/// cmd_session: analyze session logs (full report).
fn cmd_session(mode: &str, value: &str, project: Option<&str>) -> Value {
    let (files, ParsedLogs { requests, sessions }) = match collect_requests(mode, value, project) {
        Some(x) => x,
        None => return json!({"error": "no sessions found"}),
    };

    let session_count = files.len();
    let is_single_session = mode == "session";

    let (growth, compactions) = if is_single_session {
        (context_growth(&requests), detect_compactions(&requests))
    } else {
        (json!([]), json!([]))
    };

    let (startup_costs, per_session_max) = per_session_stats(&files, &sessions);

    json!({
        "sessions_analyzed": session_count,
        "total_requests": requests.len(),
        "by_tool": aggregate_by_tool(&requests),
        "top_reads": aggregate_top_reads(&requests),
        "large_operations": detect_large_ops(&requests),
        "large_responses": detect_large_responses(&requests),
        "duplicate_reads": detect_dup_reads(&requests),
        "context_growth": growth,
        "compactions": compactions,
        "per_session_max_ctx": per_session_max,
        "startup_costs": startup_costs,
        "toolsearch_breakdown": aggregate_toolsearch(&requests),
    })
}

/// cmd_session_section: emit a single section. Output JSON keeps the same key
/// names as cmd_session so token-audit-format can render it without changes.
fn cmd_session_section(
    section: &str,
    mode: &str,
    value: &str,
    project: Option<&str>,
) -> Value {
    let (files, ParsedLogs { requests, sessions }) = match collect_requests(mode, value, project) {
        Some(x) => x,
        None => return json!({"error": "no sessions found"}),
    };

    let session_count = files.len();
    let is_single_session = mode == "session";

    let mut out = json!({
        "sessions_analyzed": session_count,
        "total_requests": requests.len(),
    });
    let obj = out.as_object_mut().unwrap();

    match section {
        "by-tool" => {
            obj.insert("by_tool".into(), aggregate_by_tool(&requests));
        }
        "top-reads" => {
            obj.insert("top_reads".into(), aggregate_top_reads(&requests));
        }
        "duplicates" => {
            obj.insert("duplicate_reads".into(), detect_dup_reads(&requests));
        }
        "large-ops" => {
            obj.insert("large_operations".into(), detect_large_ops(&requests));
        }
        "large-responses" => {
            obj.insert("large_responses".into(), detect_large_responses(&requests));
        }
        "toolsearch" => {
            obj.insert(
                "toolsearch_breakdown".into(),
                aggregate_toolsearch(&requests),
            );
        }
        "startup" => {
            let (sc, ms) = per_session_stats(&files, &sessions);
            obj.insert("startup_costs".into(), sc);
            obj.insert("per_session_max_ctx".into(), ms);
        }
        "growth" => {
            // growth/compactions only meaningful for a single session
            if is_single_session {
                obj.insert("context_growth".into(), context_growth(&requests));
                obj.insert("compactions".into(), detect_compactions(&requests));
            } else {
                obj.insert("context_growth".into(), json!([]));
                obj.insert("compactions".into(), json!([]));
                obj.insert(
                    "note".into(),
                    json!("growth requires --session ID"),
                );
            }
        }
        _ => {
            return json!({
                "error": format!("unknown section: {}", section),
                "available": [
                    "by-tool", "top-reads", "duplicates",
                    "large-ops", "large-responses", "toolsearch",
                    "startup", "growth"
                ]
            });
        }
    }
    out
}

/// Aggregate delta_cc and output by tool combination key.
/// Caps to top BY_TOOL_TOP_N; remaining tools collapsed into "(others N tools)".
/// Cap protects callers that pipe raw JSON through `head` from losing tail sections.
const BY_TOOL_TOP_N: usize = 15;

fn aggregate_by_tool(requests: &[Request]) -> Value {
    let mut by_tool: HashMap<String, (i64, i64)> = HashMap::new(); // tool -> (delta_cc, total_output)
    let mut total: i64 = 0;

    for r in requests {
        let key = r.tool_key();
        let entry = by_tool.entry(key).or_default();
        entry.0 += r.delta_cc;
        entry.1 += r.output;
        total += r.delta_cc;
    }

    let mut raw: Vec<(String, i64, i64)> = by_tool
        .into_iter()
        .map(|(tool, (cc, out))| (tool, cc, out))
        .collect();
    // Tie-break on the tool key so equal costs (and the top-N cut) do not follow HashMap order.
    raw.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let make_entry = |tool: String, cc: i64, out: i64| {
        let pct = if total > 0 {
            (cc as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        let efficiency = if cc > 0 {
            (out as f64 / cc as f64 * 10.0).round() / 10.0
        } else {
            0.0
        };
        json!({
            "tool": tool,
            "cache_create": cc,
            "total_output": out,
            "pct": (pct * 10.0).round() / 10.0,
            "efficiency": efficiency,
        })
    };

    let mut items: Vec<Value> = Vec::new();
    if raw.len() <= BY_TOOL_TOP_N {
        for (tool, cc, out) in raw {
            items.push(make_entry(tool, cc, out));
        }
    } else {
        let (top, rest) = raw.split_at(BY_TOOL_TOP_N);
        for (tool, cc, out) in top {
            items.push(make_entry(tool.clone(), *cc, *out));
        }
        let others_cc: i64 = rest.iter().map(|(_, cc, _)| cc).sum();
        let others_out: i64 = rest.iter().map(|(_, _, out)| out).sum();
        items.push(make_entry(
            format!("(others {} tools)", rest.len()),
            others_cc,
            others_out,
        ));
    }

    json!(items)
}

/// Breakdown of ToolSearch queries by delta_cc cost.
/// A request's delta_cc is split evenly across its ToolSearch calls, as
/// aggregate_top_reads does for Read.
fn aggregate_toolsearch(requests: &[Request]) -> Value {
    let mut by_query: HashMap<String, (i64, i64)> = HashMap::new(); // query -> (delta_cc, count)

    for r in requests {
        let search_count = r.tools.iter().filter(|t| t.name == "ToolSearch").count();
        if search_count == 0 {
            continue;
        }
        let cc_per_search = (r.delta_cc as f64 / search_count as f64) as i64;

        for t in &r.tools {
            if t.name == "ToolSearch" {
                let query = t.query.as_deref().unwrap_or("?").to_string();
                let entry = by_query.entry(query).or_default();
                entry.0 += cc_per_search;
                entry.1 += 1;
            }
        }
    }

    let mut items: Vec<_> = by_query
        .into_iter()
        .map(|(query, (cache_create, count))| {
            json!({
                "query": query,
                "cache_create": cache_create,
                "count": count,
            })
        })
        .collect();

    items.sort_by(|a, b| {
        let a_cc = b["cache_create"].as_i64().unwrap_or(0);
        let b_cc = a["cache_create"].as_i64().unwrap_or(0);
        a_cc.cmp(&b_cc).then_with(|| str_field(a, "query").cmp(str_field(b, "query")))
    });

    json!(items)
}

/// String field of a report item, "" when missing; used as a sort tie-break.
fn str_field<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}

/// Top 10 files by Read consumption. Distribute delta_cc across reads.
fn aggregate_top_reads(requests: &[Request]) -> Value {
    let mut by_file: HashMap<String, (i64, i64)> = HashMap::new(); // file -> (count, total_cache_create)

    for r in requests {
        let read_count = r.tools.iter().filter(|t| t.name == "Read").count();
        if read_count == 0 {
            continue;
        }

        let cc_per_read = r.delta_cc as f64 / read_count as f64;

        for tool in &r.tools {
            if tool.name == "Read" {
                if let Some(fp) = tool.file_path.as_str() {
                    let entry = by_file.entry(fp.to_string()).or_default();
                    entry.0 += 1;
                    entry.1 += cc_per_read as i64;
                }
            }
        }
    }

    let mut items: Vec<_> = by_file
        .into_iter()
        .map(|(file, (count, total_cc))| {
            json!({
                "file": file,
                "count": count,
                "total_cache_create": total_cc
            })
        })
        .collect();

    items.sort_by(|a, b| {
        let a_total = b["total_cache_create"].as_i64().unwrap_or(0);
        let b_total = a["total_cache_create"].as_i64().unwrap_or(0);
        a_total.cmp(&b_total).then_with(|| str_field(a, "file").cmp(str_field(b, "file")))
    });

    items.truncate(10);
    json!(items)
}

/// Longest label kept for a request's file in the large-op/response reports.
const FILE_LABEL_MAX_CHARS: usize = 100;

/// file_path label of the request's first tool, cut to FILE_LABEL_MAX_CHARS
/// characters on a char boundary, so a multi-byte path cannot panic. None
/// when there is no tool or it has no file_path; "" when file_path is not a
/// string.
fn first_file_path(r: &Request) -> Option<String> {
    let s = r.tools.first()?.file_path.as_label()?;
    let end = s.char_indices().nth(FILE_LABEL_MAX_CHARS).map_or(s.len(), |(i, _)| i);
    Some(s[..end].to_string())
}

/// Detect large responses (output > 10000).
fn detect_large_responses(requests: &[Request]) -> Value {
    let mut items = Vec::new();

    for (idx, r) in requests.iter().enumerate() {
        if r.output > 10000 {
            let tool_key = r.tool_key();
            let file = first_file_path(r);

            let efficiency = if r.delta_cc > 0 {
                (r.output as f64 / r.delta_cc as f64 * 10.0).round() / 10.0
            } else {
                0.0
            };

            items.push(json!({
                "request_idx": idx,
                "tool": tool_key,
                "file": file,
                "output": r.output,
                "cache_create": r.delta_cc,
                "efficiency": efficiency
            }));
        }
    }

    items.sort_by(|a, b| {
        let a_output = b["output"].as_i64().unwrap_or(0);
        let b_output = a["output"].as_i64().unwrap_or(0);
        // request_idx is unique, so equal outputs stay in request order explicitly.
        a_output.cmp(&b_output).then_with(|| {
            a["request_idx"].as_u64().unwrap_or(0).cmp(&b["request_idx"].as_u64().unwrap_or(0))
        })
    });

    items.truncate(20);
    json!(items)
}

/// Detect large operations (delta_cc > 5000).
fn detect_large_ops(requests: &[Request]) -> Value {
    let mut items = Vec::new();

    // Requests come in a fixed order (files in path order), and the stable sort below keeps
    // it for items equal in cost, tool and file.
    for r in requests.iter().filter(|r| r.delta_cc > 5000) {
        let tool_key = r.tool_key();
        let file = first_file_path(r);

        items.push(json!({
            "tool": tool_key,
            "file": file,
            "cache_create": r.delta_cc
        }));
    }

    items.sort_by(|a, b| {
        let a_cc = b["cache_create"].as_i64().unwrap_or(0);
        let b_cc = a["cache_create"].as_i64().unwrap_or(0);
        a_cc.cmp(&b_cc)
            .then_with(|| str_field(a, "tool").cmp(str_field(b, "tool")))
            .then_with(|| str_field(a, "file").cmp(str_field(b, "file")))
    });

    items.truncate(20);
    json!(items)
}

/// Detect duplicate reads (count > 1).
fn detect_dup_reads(requests: &[Request]) -> Value {
    let mut by_file: HashMap<String, (i64, i64)> = HashMap::new();

    for r in requests {
        let read_count = r.tools.iter().filter(|t| t.name == "Read").count();
        if read_count == 0 {
            continue;
        }

        let cc_per_read = r.delta_cc as f64 / read_count as f64;

        for tool in &r.tools {
            if tool.name == "Read" {
                if let Some(fp) = tool.file_path.as_str() {
                    let entry = by_file.entry(fp.to_string()).or_default();
                    entry.0 += 1;
                    entry.1 += cc_per_read as i64;
                }
            }
        }
    }

    let mut items: Vec<_> = by_file
        .into_iter()
        .filter(|(_, (count, _))| *count > 1)
        .map(|(file, (count, total_cc))| {
            json!({
                "file": file,
                "count": count,
                "total_cache_create": total_cc
            })
        })
        .collect();

    items.sort_by(|a, b| {
        let a_total = b["total_cache_create"].as_i64().unwrap_or(0);
        let b_total = a["total_cache_create"].as_i64().unwrap_or(0);
        a_total.cmp(&b_total).then_with(|| str_field(a, "file").cmp(str_field(b, "file")))
    });

    json!(items)
}

/// Context growth curve (first 5 + every 50).
fn context_growth(requests: &[Request]) -> Value {
    let mut items = Vec::new();

    for (idx, r) in requests.iter().enumerate() {
        if idx < 5 || idx % 50 == 0 {
            items.push(json!({
                "request_idx": idx,
                "total_ctx": r.total_ctx()
            }));
        }
    }

    json!(items)
}

/// Detect compactions (drop > 10000).
fn detect_compactions(requests: &[Request]) -> Value {
    let mut items = Vec::new();

    for (i, window) in requests.windows(2).enumerate() {
        let before = window[0].total_ctx();
        let after = window[1].total_ctx();
        let drop = before - after;

        if drop > 10000 {
            let ratio = if before > 0 {
                after as f64 / before as f64
            } else {
                0.0
            };
            items.push(json!({
                "request_idx": i + 1, // index of the second request in the window
                "before": before,
                "after": after,
                "ratio": (ratio * 100.0).round() / 100.0,
                "compaction_cache_create": drop
            }));
        }
    }

    json!(items)
}

/// Per-session stats: first_total_ctx and max_ctx, from the stats parse_logs
/// took of each file (files without requests have none).
fn per_session_stats(files: &[PathBuf], sessions: &[SessionCtx]) -> (Value, Value) {
    let mut startup_costs = Vec::new();
    let mut per_session_max = Vec::new();

    for &SessionCtx { file_idx, first_total_ctx, max_ctx } in sessions {
        // Skip invalid data (negative or zero)
        if first_total_ctx <= 0 {
            continue;
        }

        let session_id = files[file_idx]
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|s| s.strip_suffix(".jsonl"))
            .unwrap_or("unknown")
            .to_string();

        startup_costs.push(json!({
            "session": session_id.clone(),
            "first_total_ctx": first_total_ctx
        }));

        if max_ctx > 0 {
            per_session_max.push(json!({
                "session": session_id,
                "max_ctx": max_ctx
            }));
        }
    }

    // Sort descending, ties by session id
    startup_costs.sort_by(|a, b| {
        let a_ft = b["first_total_ctx"].as_i64().unwrap_or(0);
        let b_ft = a["first_total_ctx"].as_i64().unwrap_or(0);
        a_ft.cmp(&b_ft).then_with(|| str_field(a, "session").cmp(str_field(b, "session")))
    });

    per_session_max.sort_by(|a, b| {
        let a_mx = b["max_ctx"].as_i64().unwrap_or(0);
        let b_mx = a["max_ctx"].as_i64().unwrap_or(0);
        a_mx.cmp(&b_mx).then_with(|| str_field(a, "session").cmp(str_field(b, "session")))
    });

    (json!(startup_costs), json!(per_session_max))
}

/// cmd_compare: compare two sessions.
fn cmd_compare(id1: &str, id2: &str) -> Value {
    let all_files = find_jsonl_files();
    let files1 = find_session_files(&all_files, id1);
    let files2 = find_session_files(&all_files, id2);

    let file1 = files1.first().map(|f| &f.path);
    let file2 = files2.first().map(|f| &f.path);

    if file1.is_none() || file2.is_none() {
        return json!({"error": "one or both sessions not found"});
    }

    let reqs1 = parse_file(file1.unwrap(), 0).requests;
    let reqs2 = parse_file(file2.unwrap(), 0).requests;

    let first_ctx1 = reqs1.first().map(|r| r.total_ctx()).unwrap_or(0);
    let first_ctx2 = reqs2.first().map(|r| r.total_ctx()).unwrap_or(0);

    json!({
        "session1": {
            "id": id1,
            "first_ctx": first_ctx1,
            "by_tool": tool_breakdown(&reqs1)
        },
        "session2": {
            "id": id2,
            "first_ctx": first_ctx2,
            "by_tool": tool_breakdown(&reqs2)
        },
        "diff_first_ctx": first_ctx2 - first_ctx1
    })
}

/// Tool breakdown (without pct) for compare.
fn tool_breakdown(requests: &[Request]) -> Value {
    // BTreeMap: serde_json keeps insertion order (preserve_order), so a HashMap here would
    // print the tools in a different order on every run.
    let mut map: BTreeMap<String, i64> = BTreeMap::new();
    for r in requests {
        *map.entry(r.tool_key()).or_default() += r.delta_cc;
    }
    json!(map)
}

// ============================================================================
// Public entry point
// ============================================================================

/// Main entry point for token-audit command.
pub fn run(args: &[String]) {
    match args.first().map(|s| s.as_str()) {
        Some("static") => {
            let result = cmd_static();
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        Some("session") => {
            // Optional first positional: section name (e.g. "by-tool"). Bare
            // `session` keeps the old full-report behaviour.
            let rest = &args[1..];
            let (section, opts) = match rest.first() {
                Some(s) if !s.starts_with("--") => (Some(s.as_str()), &rest[1..]),
                _ => (None, rest),
            };
            let sa = parse_session_args(opts);
            let result = match section {
                None => cmd_session(&sa.mode, &sa.value, sa.project.as_deref()),
                Some(sec) => cmd_session_section(
                    sec,
                    &sa.mode,
                    &sa.value,
                    sa.project.as_deref(),
                ),
            };
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        Some("compare") => {
            if args.len() < 3 {
                eprintln!("Usage: dotcli token-audit compare <id1> <id2>");
                std::process::exit(1);
            }
            let result = cmd_compare(&args[1], &args[2]);
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        Some("all") => {
            let sa = if args.len() > 1 {
                parse_session_args(&args[1..])
            } else {
                SessionArgs { mode: "last".to_string(), value: "10".to_string(), project: None }
            };
            let static_result = cmd_static();
            let session_result = cmd_session(&sa.mode, &sa.value, sa.project.as_deref());
            print!("{}", super::token_audit_format::format_static(&static_result));
            println!();
            print!("{}", super::token_audit_format::format_session(&session_result));
        }
        _ => {
            eprintln!("Usage: dotcli token-audit <static|session|all|compare>");
            eprintln!("  dotcli token-audit static");
            eprintln!("  dotcli token-audit session [SECTION] [--all|--last N|--session ID] [--project PATH]");
            eprintln!("    SECTION: by-tool|top-reads|duplicates|large-ops|large-responses|toolsearch|startup|growth");
            eprintln!("    (omit SECTION for full report)");
            eprintln!("  dotcli token-audit all [--last N|--session ID] [--project PATH]  (default: --last 10)");
            eprintln!("  dotcli token-audit compare <id1> <id2>");
            std::process::exit(1);
        }
    }
}

struct SessionArgs {
    mode: String,
    value: String,
    project: Option<String>,
}

fn parse_session_args(args: &[String]) -> SessionArgs {
    let mut mode = "all".to_string();
    let mut value = String::new();
    let mut project = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--all" => {
                mode = "all".to_string();
                i += 1;
            }
            "--last" => {
                mode = "last".to_string();
                value = args.get(i + 1).cloned().unwrap_or_default();
                i += 2;
            }
            "--session" => {
                mode = "session".to_string();
                value = args.get(i + 1).cloned().unwrap_or_default();
                i += 2;
            }
            "--project" => {
                project = Some(args.get(i + 1).cloned().unwrap_or_default());
                i += 2;
            }
            _ => {
                eprintln!("Unknown session option: {}", args[i]);
                std::process::exit(1);
            }
        }
    }
    SessionArgs { mode, value, project }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dotcli-token-audit-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"{}\n").unwrap();
    }

    /// Directory symlink, or a junction when symlinks need privileges we lack
    /// (both are name-surrogate reparse points, which is_symlink() reports).
    #[cfg(windows)]
    fn link_dir(target: &Path, link: &Path) -> std::io::Result<()> {
        if std::os::windows::fs::symlink_dir(target, link).is_ok() {
            return Ok(());
        }
        let status = std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(link)
            .arg(target)
            .stdout(std::process::Stdio::null())
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other("mklink /J failed"))
        }
    }
    #[cfg(unix)]
    fn link_dir(target: &Path, link: &Path) -> std::io::Result<()> {
        std::os::unix::fs::symlink(target, link)
    }

    #[test]
    fn encode_project_turns_every_non_alphanumeric_into_dash() {
        let enc = encode_project_prefixes(r"C:\Main\Project\AnalyzerCH\.claude\worktrees\builder-b4");
        assert_eq!(enc[0], "C--Main-Project-AnalyzerCH--claude-worktrees-builder-b4");
        assert_eq!(
            encode_project_prefixes(r"C:\Main\Project\Foo"),
            vec!["C--Main-Project-Foo", "C--Main--Project--Foo"]
        );
        assert_eq!(encode_project_prefixes(r"C:\Users\me\v2.5_x y")[0], "C--Users-me-v2-5-x-y");
    }

    #[test]
    fn encode_project_accepts_each_path_form() {
        let want = encode_project_prefixes(r"C:\Main\Foo");
        for p in ["C:/Main/Foo", "/c/Main/Foo", "c:\\Main\\Foo\\", "C:/Main//Foo/", "/c/Main/Foo/"] {
            assert_eq!(encode_project_prefixes(p), want, "{p}");
        }
    }

    #[test]
    fn encode_project_drive_alone_is_the_drive_root() {
        for p in ["C:", "C:/", r"C:\", "/c", "/c/", "c:"] {
            assert_eq!(encode_project_prefixes(p), vec!["C--"], "{p}");
        }
    }

    #[test]
    fn encode_project_rejects_other_input_without_panicking() {
        for p in ["", "/", "C", "C:foo", "/é/x", "é:/x", "/cc/x", "relative/dir", "日本"] {
            assert!(encode_project_prefixes(p).is_empty(), "{p}");
        }
    }

    #[test]
    fn project_match_takes_the_project_and_its_worktrees_only() {
        let enc = &encode_project_prefixes(r"C:\Main\Project\AnalyzerCH")[0];
        assert!(project_dir_matches("C--Main-Project-AnalyzerCH", enc));
        assert!(project_dir_matches("C--Main-Project-AnalyzerCH--claude-worktrees-builder-b4", enc));
        assert!(!project_dir_matches("C--Main-Project-AnalyzerCH-design", enc));
        assert!(!project_dir_matches("C--Main-Project-AnalyzerCH2", enc));

        let enc = &encode_project_prefixes(r"C:\Main\Project\ch-data")[0];
        assert!(project_dir_matches("C--Main-Project-ch-data", enc));
        assert!(!project_dir_matches("C--Main-Project-ch-data-collector", enc));
        assert!(!project_dir_matches("C--Main-Project-ch-data-model", enc));

        let enc = &encode_project_prefixes(r"C:\Main\Project")[0];
        assert!(!project_dir_matches("C--Main-Project-AnalyzerCH", enc));
    }

    #[test]
    fn filter_by_project_keeps_old_convention_worktrees_and_subagents() {
        let file = |dir: &str, rest: &str| JsonlFile {
            path: PathBuf::from(format!("C:/h/.claude/projects/{dir}/{rest}")),
            mtime: None,
        };
        let files = vec![
            file("C--Main-Project-dotfiles", "a.jsonl"),
            file("C--Main--Project--dotfiles", "b.jsonl"),
            file("C--Main-Project-dotfiles--claude-worktrees-x", "c.jsonl"),
            file("C--Main-Project-dotfiles", "s1/subagents/d.jsonl"),
            file("C--Main-Project-dotfiles-idle-notify2", "e.jsonl"),
            file("C--Main-Project-other", "f.jsonl"),
        ];
        let names: Vec<String> = filter_by_project(&files, r"C:\Main\Project\dotfiles")
            .iter()
            .map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["a.jsonl", "b.jsonl", "c.jsonl", "d.jsonl"]);
    }

    #[test]
    fn discover_dedups_by_basename_first_wins_and_sorts_by_path() {
        let root = scratch("dedup");
        let a = root.join("a");
        let b = root.join("b");
        touch(&a.join("projects/P2/s2.jsonl"));
        touch(&a.join("projects/P1/s1.jsonl"));
        touch(&a.join("projects/P1/s1/subagents/agent-x.jsonl"));
        touch(&a.join("projects/P1/notes.txt"));
        touch(&b.join("projects/P0/s1.jsonl")); // same basename, later profile
        touch(&b.join("projects/P0/s3.jsonl"));

        let got = discover_jsonl(&[a.clone(), b.clone(), root.join("missing")]);
        let paths: Vec<PathBuf> = got.iter().map(|f| f.path.clone()).collect();
        let mut sorted = paths.clone();
        sorted.sort();
        assert_eq!(paths, sorted, "paths must come back in path order");
        assert_eq!(paths.len(), 4);
        assert!(paths.contains(&a.join("projects").join("P1").join("s1.jsonl")));
        assert!(!paths.contains(&b.join("projects").join("P0").join("s1.jsonl")));
        assert!(got.iter().all(|f| f.mtime.is_some()));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn discover_walks_a_symlinked_projects_dir_once_and_keeps_original_paths() {
        let root = scratch("symlink");
        let a = root.join("a");
        let b = root.join("b");
        touch(&a.join("projects/P1/s1.jsonl"));
        fs::create_dir_all(&b).unwrap();
        if link_dir(&a.join("projects"), &b.join("projects")).is_err() {
            // Creating symlinks needs Developer Mode or admin on Windows.
            eprintln!("skipped: cannot create a directory symlink here");
            let _ = fs::remove_dir_all(&root);
            return;
        }
        // Before the basename dedup: the shared tree is listed once, not twice.
        let walked = walk_projects_dirs(&[b.clone(), a.clone()]);
        assert_eq!(walked.len(), 1, "shared projects tree walked more than once");
        // b first: its spelling is kept even though a resolves to the same tree.
        let got = discover_jsonl(&[b.clone(), a.clone()]);
        let paths: Vec<PathBuf> = got.iter().map(|f| f.path.clone()).collect();
        assert_eq!(paths, vec![b.join("projects").join("P1").join("s1.jsonl")]);
        assert!(got[0].mtime.is_some());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn latest_breaks_mtime_ties_by_input_order_and_skips_unknown_times() {
        let t = |s: u64| Some(UNIX_EPOCH + Duration::from_secs(s));
        let f = |p: &str, m| JsonlFile { path: PathBuf::from(p), mtime: m };
        let files = vec![f("a", t(5)), f("b", t(9)), f("c", t(5)), f("d", None), f("e", t(5))];
        let got: Vec<PathBuf> = find_latest_files(&files, 3).into_iter().map(|f| f.path).collect();
        assert_eq!(got, vec![PathBuf::from("b"), PathBuf::from("a"), PathBuf::from("c")]);
        assert_eq!(find_latest_files(&files, 10).len(), 4);
    }

    /// One assistant log line: `usage` is (input, output, cache_read, cache_create).
    fn log_line(rid: &str, content: Value, usage: (i64, i64, i64, i64)) -> String {
        json!({
            "type": "assistant",
            "requestId": rid,
            "message": {
                "role": "assistant",
                "content": content,
                "usage": {
                    "input_tokens": usage.0,
                    "output_tokens": usage.1,
                    "cache_read_input_tokens": usage.2,
                    "cache_creation_input_tokens": usage.3,
                }
            }
        })
        .to_string()
    }

    fn text_block() -> Value {
        json!([{"type": "text", "text": "x"}])
    }

    fn tool_block(id: &str, name: &str, input: Value) -> Value {
        json!([{"type": "tool_use", "id": id, "name": name, "input": input}])
    }

    fn write_log(path: &Path, lines: &[String]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut body = lines.join("\n");
        body.push('\n');
        fs::write(path, body).unwrap();
    }

    #[test]
    fn lines_of_one_request_are_merged_with_all_their_tools() {
        let root = scratch("merge-lines");
        let log = root.join("s.jsonl");
        let u = (10, 5, 100, 50);
        write_log(&log, &[
            log_line("r1", text_block(), u),
            json!({"type": "user", "message": {"role": "user", "content": "tool result"}}).to_string(),
            log_line("r1", tool_block("t1", "Read", json!({"file_path": "A"})), u),
            log_line("r1", tool_block("t2", "Read", json!({"file_path": "B"})), u),
        ]);

        let reqs = parse_logs(&[log]).requests;
        assert_eq!(reqs.len(), 1);
        let names: Vec<&str> = reqs[0].tools.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["Read", "Read"]);
        assert_eq!(reqs[0].tool_key(), "Read");

        let top = aggregate_top_reads(&reqs);
        let count_of = |f: &str| {
            top.as_array()
                .unwrap()
                .iter()
                .find(|x| x["file"] == f)
                .map(|x| x["count"].as_i64().unwrap())
        };
        assert_eq!(count_of("A"), Some(1));
        assert_eq!(count_of("B"), Some(1));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn repeated_tool_use_id_is_kept_once_and_last_line_usage_wins() {
        let root = scratch("merge-dedup");
        let log = root.join("s.jsonl");
        write_log(&log, &[
            log_line("r1", tool_block("t1", "Bash", json!({"command": "ls"})), (1, 2, 3, 4)),
            log_line("r1", tool_block("t1", "Bash", json!({"command": "ls"})), (1, 2, 3, 4)),
            log_line("r1", tool_block("t2", "Bash", json!({"command": "pwd"})), (10, 20, 30, 40)),
            // A block without an id cannot be matched, so it is always kept.
            log_line("r1", json!([{"type": "tool_use", "name": "Grep", "input": {}}]), (11, 21, 31, 41)),
        ]);

        let reqs = parse_logs(&[log]).requests;
        assert_eq!(reqs.len(), 1);
        let ids: Vec<Option<&str>> = reqs[0].tools.iter().map(|t| t.id.as_deref()).collect();
        assert_eq!(ids, vec![Some("t1"), Some("t2"), None]);
        let r = &reqs[0];
        assert_eq!((r.input, r.output, r.cache_read, r.cache_create), (11, 21, 31, 41));
        assert_eq!(r.delta_cc, 11 + 31 + 41);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn request_repeated_in_a_later_file_keeps_the_first_and_deltas_stay_per_file() {
        let root = scratch("cross-file");
        let f1 = root.join("a.jsonl");
        let f2 = root.join("b.jsonl");
        // total_ctx = input + cache_read + cache_create
        let r1 = (0, 1, 0, 1000); // 1000
        let r2 = (0, 1, 1000, 500); // 1500
        let r3 = (0, 1, 1500, 1500); // 3000
        let r2_copy = (0, 1, 1000, 700); // 1700: a fork's copy that differs
        let r4 = (0, 1, 1700, 300); // 2000
        write_log(&f1, &[
            log_line("r1", text_block(), r1),
            log_line("r2", text_block(), r2),
            log_line("r3", text_block(), r3),
        ]);
        write_log(&f2, &[
            log_line("r1", text_block(), r1),
            log_line("r2", text_block(), r2_copy),
            log_line("r4", text_block(), r4),
        ]);

        let reqs = parse_logs(&[f1, f2]).requests;
        let ids: Vec<&str> = reqs.iter().map(|r| r.req_id.as_str()).collect();
        assert_eq!(ids, vec!["r1", "r2", "r3", "r4"]);
        assert!(reqs[..3].iter().all(|r| r.file_idx == 0), "first file must win");
        assert_eq!(reqs[3].file_idx, 1);
        assert_eq!(reqs[1].total_ctx(), 1500, "r2 keeps the first file's usage");

        assert_eq!(reqs[0].delta_cc, 1000);
        assert_eq!(reqs[1].delta_cc, 1500 - 1000);
        assert_eq!(reqs[2].delta_cc, 3000 - 1500);
        // r4 follows file 2's own copy of r2, not r3 of the merged list.
        assert_eq!(reqs[3].delta_cc, 2000 - 1700);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn toolsearch_cost_is_split_across_the_searches_of_one_request() {
        let root = scratch("toolsearch");
        let log = root.join("s.jsonl");
        let u = (0, 1, 0, 1000);
        write_log(&log, &[
            log_line("r1", tool_block("t1", "ToolSearch", json!({"query": "q1"})), u),
            log_line("r1", tool_block("t2", "ToolSearch", json!({"query": "q2"})), u),
        ]);

        let reqs = parse_logs(&[log]).requests;
        assert_eq!(reqs[0].delta_cc, 1000);
        let out = aggregate_toolsearch(&reqs);
        let items = out.as_array().unwrap();
        assert_eq!(items.len(), 2);
        for q in ["q1", "q2"] {
            let it = items.iter().find(|x| x["query"] == q).unwrap();
            assert_eq!(it["cache_create"], 500, "query {}", q);
            assert_eq!(it["count"], 1, "query {}", q);
        }
        let _ = fs::remove_dir_all(&root);
    }

    // ------------------------------------------------------------------
    // Reference parser: the serde_json::Value reading the typed parser
    // replaced. parse_line must give the same Request for every line.
    // ------------------------------------------------------------------

    fn value_str_field(v: Option<&Value>) -> StrField {
        match v {
            None => StrField::Missing,
            Some(Value::String(s)) => StrField::Str(s.clone()),
            Some(_) => StrField::NotStr,
        }
    }

    fn value_parse_line(line: &str) -> Option<Request> {
        let v: Value = serde_json::from_str(line).ok()?;
        let has_type = v.get("type").and_then(|x| x.as_str()) == Some("assistant");
        let has_role = v["message"]["role"].as_str() == Some("assistant");
        let has_usage = v["message"]["usage"].is_object();
        let req_id = v.get("requestId").and_then(|x| x.as_str())?.to_string();
        if !((has_type || has_role) && has_usage) {
            return None;
        }
        let mut tools = Vec::new();
        if let Some(arr) = v["message"]["content"].as_array() {
            for item in arr {
                if item["type"].as_str() != Some("tool_use") {
                    continue;
                }
                if let Some(name) = item["name"].as_str() {
                    let input = item.get("input").cloned().unwrap_or(Value::Null);
                    tools.push(ToolUse {
                        id: item["id"].as_str().map(str::to_string),
                        name: name.to_string(),
                        file_path: value_str_field(input.get("file_path")),
                        query: input.get("query").and_then(|x| x.as_str()).map(str::to_string),
                    });
                }
            }
        }
        let usage = &v["message"]["usage"];
        Some(Request {
            req_id,
            tools,
            input: usage["input_tokens"].as_i64().unwrap_or(0),
            output: usage["output_tokens"].as_i64().unwrap_or(0),
            cache_read: usage["cache_read_input_tokens"].as_i64().unwrap_or(0),
            cache_create: usage["cache_creation_input_tokens"].as_i64().unwrap_or(0),
            delta_cc: 0,
            file_idx: 0,
        })
    }

    /// The former parse_file: every line through Value, no prefilter.
    fn value_parse_file(path: &Path, file_idx: usize) -> Vec<Request> {
        let mut requests: Vec<Request> = Vec::new();
        let mut id_index: HashMap<String, usize> = HashMap::new();
        let f = fs::File::open(path).unwrap();
        for line in BufReader::new(f).lines().flatten() {
            let Some(mut req) = value_parse_line(&line) else { continue };
            req.file_idx = file_idx;
            match id_index.get(&req.req_id) {
                Some(&idx) => requests[idx].merge_line(req),
                None => {
                    id_index.insert(req.req_id.clone(), requests.len());
                    requests.push(req);
                }
            }
        }
        compute_deltas(&mut requests);
        requests
    }

    fn typed_parse_line(line: &str) -> Option<Request> {
        let mut buf = line.as_bytes().to_vec();
        parse_line(&mut buf, &mut simd_json::Buffers::default())
    }

    #[test]
    fn typed_line_parse_matches_value_parse() {
        let lines = [
            // plain assistant line with a Read and a ToolSearch
            r#"{"type":"assistant","requestId":"r","message":{"role":"assistant","content":[{"type":"text","text":"x"},{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"C:\\a\\b.rs","limit":5}},{"type":"tool_use","id":"t2","name":"ToolSearch","input":{"query":"select:Read"}}],"usage":{"input_tokens":1,"output_tokens":2,"cache_read_input_tokens":3,"cache_creation_input_tokens":4}}}"#,
            // usage fields: missing, float, string, null, negative, beyond i64
            r#"{"type":"assistant","requestId":"r","message":{"usage":{"input_tokens":1.0,"output_tokens":"5","cache_read_input_tokens":null}}}"#,
            r#"{"type":"assistant","requestId":"r","message":{"usage":{"input_tokens":-7,"output_tokens":18446744073709551615,"cache_read_input_tokens":9223372036854775807,"cache_creation_input_tokens":1e3}}}"#,
            r#"{"type":"assistant","requestId":"r","message":{"usage":{}}}"#,
            // usage not an object, or missing: not a request
            r#"{"type":"assistant","requestId":"r","message":{"usage":[1,2]}}"#,
            r#"{"type":"assistant","requestId":"r","message":{"usage":null}}"#,
            r#"{"type":"assistant","requestId":"r","message":{"content":[]}}"#,
            // role alone, type alone, neither
            r#"{"type":"progress","requestId":"r","message":{"role":"assistant","usage":{}}}"#,
            r#"{"type":"assistant","requestId":"r","message":{"role":"user","usage":{}}}"#,
            r#"{"type":"user","requestId":"r","message":{"role":"user","usage":{}}}"#,
            r#"{"type":5,"requestId":"r","message":{"role":["assistant"],"usage":{}}}"#,
            // requestId missing, not a string, or nested only
            r#"{"type":"assistant","message":{"usage":{},"requestId":"r"}}"#,
            r#"{"type":"assistant","requestId":7,"message":{"usage":{}}}"#,
            r#"{"type":"assistant","requestId":null,"message":{"usage":{}}}"#,
            // message not an object
            r#"{"type":"assistant","requestId":"r","message":"hi"}"#,
            r#"{"type":"assistant","requestId":"r","message":null}"#,
            // content: string, object, items of other types
            r#"{"type":"assistant","requestId":"r","message":{"content":"text","usage":{}}}"#,
            r#"{"type":"assistant","requestId":"r","message":{"content":{"type":"tool_use","name":"Read"},"usage":{}}}"#,
            r#"{"type":"assistant","requestId":"r","message":{"content":[1,"s",null,[{"type":"tool_use","name":"X"}],{"type":"tool_use","name":"Read","input":{"file_path":"A"}}],"usage":{}}}"#,
            // tool fields of other types
            r#"{"type":"assistant","requestId":"r","message":{"content":[{"type":"tool_use","name":5,"input":{}},{"type":"tool_use","input":{}},{"type":"tool_use","id":3,"name":"Bash"},{"type":"tool_use","id":"t","name":"Read","input":"x"},{"type":"tool_use","name":"Read","input":{"file_path":null}},{"type":"tool_use","name":"Read","input":{"file_path":{"p":1}}},{"type":"tool_use","name":"ToolSearch","input":{"query":["a"]}}],"usage":{}}}"#,
            // repeated keys: the last value wins, as in Value
            r#"{"type":"assistant","requestId":"a","requestId":"b","message":{"usage":{"input_tokens":1,"input_tokens":2},"role":"x","content":[{"type":"text","type":"tool_use","name":"N","name":"M","input":{"file_path":"1","file_path":5}}]}}"#,
            r#"{"type":"assistant","requestId":"a","message":{"usage":{}},"message":{"role":"user"}}"#,
            // escapes and non-ASCII in kept strings
            r#"{"type":"assistant","requestId":"r\u0031","message":{"content":[{"type":"tool_use","id":"t","name":"Read","input":{"file_path":"C:/\u00e9/\"q\"/特許庁/\ud83d\ude00.md"}}],"usage":{}}}"#,
            // escaped key spelling still parses (only the prefilter misses it)
            r#"{"type":"assistant","request\u0049d":"r","message":{"usage":{}}}"#,
            // deep nesting in skipped fields
            r#"{"type":"assistant","requestId":"r","x":[[[[[{"a":{"b":[1,2,{"c":null}]}}]]]]],"message":{"usage":{},"stop_reason":null}}"#,
            // whitespace, CR, and broken lines
            "{\"type\":\"assistant\",\"requestId\":\"r\",\"message\":{\"usage\":{}}}\r",
            " \t{\"type\":\"assistant\" , \"requestId\" : \"r\" , \"message\" : { \"usage\" : { } } } ",
            r#"{"type":"assistant","requestId":"r","message":{"usage":{}}"#,
            r#"{"type":"assistant","requestId":"r","message":{"usage":{}}} x"#,
            r#"{"type":"assistant","requestId":"r","message":{"usage":{}}}{}"#,
            r#"[{"type":"assistant","requestId":"r","message":{"usage":{}}}]"#,
            r#""requestId""#,
            "",
        ];
        for line in lines {
            assert_eq!(typed_parse_line(line), value_parse_line(line), "line: {}", line);
        }
        // The cases above must not all be rejections.
        let parsed = lines.iter().filter(|l| typed_parse_line(l).is_some()).count();
        assert!(parsed >= 15, "only {} lines parsed", parsed);
        let first = typed_parse_line(lines[0]).unwrap();
        assert_eq!(first.tools[0].file_path, StrField::Str("C:\\a\\b.rs".into()));
        assert_eq!(first.tools[1].query.as_deref(), Some("select:Read"));
    }

    #[test]
    fn non_string_file_path_labels_large_ops_with_an_empty_string() {
        let reqs = vec![
            typed_parse_line(r#"{"type":"assistant","requestId":"a","message":{"content":[{"type":"tool_use","name":"Read","input":{"file_path":null}}],"usage":{"cache_creation_input_tokens":6000}}}"#).unwrap(),
            typed_parse_line(r#"{"type":"assistant","requestId":"b","message":{"content":[{"type":"tool_use","name":"Bash","input":{}}],"usage":{"cache_creation_input_tokens":12000}}}"#).unwrap(),
        ];
        let mut reqs = reqs;
        compute_deltas(&mut reqs);
        let ops = detect_large_ops(&reqs);
        let file_of = |tool: &str| {
            ops.as_array().unwrap().iter().find(|x| x["tool"] == tool).unwrap()["file"].clone()
        };
        assert_eq!(file_of("Read"), json!(""));
        assert_eq!(file_of("Bash"), Value::Null);
        // Neither counts as a Read of some file.
        assert_eq!(aggregate_top_reads(&reqs), json!([]));
    }

    /// A request whose only tool is a Read of `path`, large in both output
    /// and delta_cc.
    fn large_read_of(path: &str) -> Request {
        let line = log_line("r", tool_block("t", "Read", json!({"file_path": path})), (0, 20000, 0, 9000));
        let mut reqs = vec![typed_parse_line(&line).unwrap()];
        compute_deltas(&mut reqs);
        reqs.pop().unwrap()
    }

    #[test]
    fn multibyte_file_paths_are_cut_on_a_char_boundary() {
        // 120 bytes but 40 chars: kept whole (byte 100 is mid-char).
        let short = "あ".repeat(40);
        // 150 chars: cut to the first 100 chars.
        let long = "あ".repeat(150);
        for (path, want) in [(short.clone(), short.clone()), (long.clone(), "あ".repeat(100))] {
            let reqs = vec![large_read_of(&path)];
            assert_eq!(first_file_path(&reqs[0]).as_deref(), Some(want.as_str()));
            assert_eq!(detect_large_responses(&reqs)[0]["file"], json!(want));
            assert_eq!(detect_large_ops(&reqs)[0]["file"], json!(want));
        }
        // ASCII labels are cut at 100 bytes as before.
        let ascii = "a".repeat(130);
        assert_eq!(first_file_path(&large_read_of(&ascii)).unwrap(), "a".repeat(100));
    }

    #[test]
    fn lines_without_the_request_id_key_are_skipped_unparsed() {
        let root = scratch("prefilter");
        let log = root.join("s.jsonl");
        let u = (0, 1, 0, 100);
        write_log(&log, &[
            // Would be a request if parsed, but the key is spelled with an
            // escape, so the byte prefilter drops it.
            r#"{"type":"assistant","request\u0049d":"r0","message":{"usage":{}}}"#.to_string(),
            json!({"type": "user", "message": {"role": "user", "content": "x"}}).to_string(),
            log_line("r1", text_block(), u),
            "not json \"requestId\"".to_string(),
        ]);
        let reqs = parse_logs(&[log]).requests;
        let ids: Vec<&str> = reqs.iter().map(|r| r.req_id.as_str()).collect();
        assert_eq!(ids, vec!["r1"]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn last_line_without_newline_and_crlf_lines_are_read() {
        let root = scratch("line-ends");
        let log = root.join("s.jsonl");
        let body = format!(
            "{}\r\n{}",
            log_line("r1", text_block(), (0, 1, 0, 100)),
            log_line("r2", text_block(), (0, 1, 100, 50)),
        );
        fs::write(&log, body).unwrap();
        let reqs = parse_logs(&[log]).requests;
        let ids: Vec<&str> = reqs.iter().map(|r| r.req_id.as_str()).collect();
        assert_eq!(ids, vec!["r1", "r2"]);
        assert_eq!(reqs[1].delta_cc, 50);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn session_stats_come_from_each_file_before_the_cross_file_dedup() {
        let root = scratch("session-stats");
        let f1 = root.join("a.jsonl");
        let f2 = root.join("b.jsonl");
        let f3 = root.join("c.jsonl");
        write_log(&f1, &[
            log_line("r1", text_block(), (0, 1, 0, 1000)),  // 1000
            log_line("r2", text_block(), (0, 1, 0, 400)),   // first line of r2
            log_line("r2", text_block(), (0, 1, 1000, 2000)), // r2 = 3000
            log_line("r3", text_block(), (0, 1, 1500, 500)), // 2000
        ]);
        // b repeats a's history: its r1/r2 are dropped from the requests
        // but still count for b's own stats.
        write_log(&f2, &[
            log_line("r1", text_block(), (0, 1, 0, 1000)),
            log_line("r2", text_block(), (0, 1, 1000, 4000)), // 5000
            log_line("r4", text_block(), (0, 1, 2000, 0)),    // 2000
        ]);
        write_log(&f3, &[json!({"type": "user"}).to_string()]);
        let files = vec![f1, f2, f3];

        let parsed = parse_logs(&files);
        assert_eq!(parsed.requests.len(), 4);
        assert_eq!(
            parsed.sessions,
            vec![
                SessionCtx { file_idx: 0, first_total_ctx: 1000, max_ctx: 3000 },
                SessionCtx { file_idx: 1, first_total_ctx: 1000, max_ctx: 5000 },
            ]
        );
        // Same values as parsing each file alone.
        for s in &parsed.sessions {
            let alone = parse_logs(&files[s.file_idx..=s.file_idx]).requests;
            assert_eq!(s.first_total_ctx, alone[0].total_ctx());
            assert_eq!(s.max_ctx, alone.iter().map(Request::total_ctx).max().unwrap());
        }

        let (startup, max) = per_session_stats(&files, &parsed.sessions);
        assert_eq!(
            startup,
            json!([
                {"session": "a", "first_total_ctx": 1000},
                {"session": "b", "first_total_ctx": 1000},
            ])
        );
        assert_eq!(
            max,
            json!([
                {"session": "b", "max_ctx": 5000},
                {"session": "a", "max_ctx": 3000},
            ])
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// Compares the typed parser with the Value parser on every session log
    /// of this machine (read only). Run on demand:
    /// cargo test --release token_audit::tests::real_logs -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_logs_parse_the_same_as_the_value_parser() {
        let cutoff = SystemTime::now() - std::time::Duration::from_secs(600);
        // Files still being written could change between the two reads.
        let files: Vec<PathBuf> = find_jsonl_files()
            .into_iter()
            .filter(|f| f.mtime.map_or(false, |t| t < cutoff))
            .map(|f| f.path)
            .collect();
        let mismatched: Vec<String> = files
            .par_iter()
            .enumerate()
            .filter_map(|(i, f)| {
                let typed = parse_file(f, i).requests;
                let value = value_parse_file(f, i);
                (typed != value).then(|| normalize_path(f))
            })
            .collect();
        let requests: usize = parse_logs(&files).requests.len();
        eprintln!("{} files, {} requests, {} mismatched", files.len(), requests, mismatched.len());
        assert!(mismatched.is_empty(), "mismatched: {:?}", mismatched);
    }
}
