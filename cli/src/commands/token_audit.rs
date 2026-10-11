//! Token consumption audit. Replaces bin/token-audit (bash + jq).
//! Direct std::fs usage (no process spawn) for performance on Windows.

use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Assistant message extracted from JSONL line. Equivalent to bash JQ_DEDUP.
#[derive(Debug, Clone)]
struct Request {
    req_id: String,
    tools: Vec<ToolUse>,
    input: i64,
    output: i64,
    cache_read: i64,
    cache_create: i64,
    /// total_ctx growth from the previous request of the same file
    /// (populated by parse_requests).
    delta_cc: i64,
    /// Index of the file the requestId was first seen in.
    file_idx: usize,
}

#[derive(Debug, Clone)]
struct ToolUse {
    /// tool_use block id; None when the log omits it (never deduplicated).
    id: Option<String>,
    name: String,
    input: Value, // .input field (for file_path extraction)
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

/// Encode a project path into project directory name prefixes (both conventions).
/// e.g. C:\Main\Project\Foo -> ["C--Main-Project-Foo", "C--Main--Project--Foo"]
fn encode_project_prefixes(project_path: &str) -> Vec<String> {
    // Normalize to forward slashes, strip trailing slash
    let normalized = project_path.replace('\\', "/");
    let normalized = normalized.trim_end_matches('/');

    // Extract drive letter and components
    // Handle: /c/Main/Project/Foo, C:/Main/Project/Foo, C:\Main\Project\Foo
    let (drive, components) = if normalized.starts_with('/') && normalized.len() >= 2 {
        // Unix-style: /c/Main/Project/Foo
        let drive = normalized[1..2].to_uppercase();
        let rest = &normalized[3..]; // skip /c/
        let comps: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
        (drive, comps)
    } else if normalized.len() >= 2 && normalized.as_bytes()[1] == b':' {
        // Windows-style: C:/Main/Project/Foo
        let drive = normalized[..1].to_uppercase();
        let rest = &normalized[3..]; // skip C:/
        let comps: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
        (drive, comps)
    } else {
        return Vec::new();
    };

    if components.is_empty() {
        return Vec::new();
    }

    // New convention: C--Main-Project-Foo (single dash separator)
    let new_conv = format!("{}--{}", drive, components.join("-"));
    // Old convention: C--Main--Project--Foo (double dash separator)
    let old_conv = format!("{}--{}", drive, components.join("--"));

    if new_conv == old_conv {
        vec![new_conv]
    } else {
        vec![new_conv, old_conv]
    }
}

/// Filter JSONL files to those belonging to a project (prefix match on parent dir name).
/// Includes worktree sessions (e.g. C--Main-Project-Foo--claude-worktrees-*).
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
                    if prefixes.iter().any(|prefix| name.starts_with(prefix)) {
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

/// Parse JSONL files into one Request per requestId, with delta_cc set.
/// Each file is parsed on its own (see parse_file) and its deltas are taken
/// over that file's own request sequence. A requestId already seen in an
/// earlier file (a forked or resumed session repeats its history) is skipped:
/// the first file's version and position win.
fn parse_requests(files: &[PathBuf]) -> Vec<Request> {
    let mut requests: Vec<Request> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for (file_idx, file_path) in files.iter().enumerate() {
        for req in parse_file(file_path, file_idx) {
            if !seen.contains(&req.req_id) {
                seen.insert(req.req_id.clone());
                requests.push(req);
            }
        }
    }

    requests
}

/// Parse one session log. Lines sharing a requestId are merged into one
/// Request at the position of its first line (see Request::merge_line), then
/// delta_cc is computed over the file's requests in that order.
fn parse_file(file_path: &Path, file_idx: usize) -> Vec<Request> {
    let mut requests: Vec<Request> = Vec::new();
    let mut id_index: HashMap<String, usize> = HashMap::new();

    let f = match fs::File::open(file_path) {
        Ok(f) => f,
        Err(_) => return requests,
    };
    for line in BufReader::new(f).lines().flatten() {
        let val = match serde_json::from_str::<Value>(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if !is_assistant_message(&val) {
            continue;
        }
        let mut req = match extract_request(&val) {
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

    compute_deltas(&mut requests);
    requests
}

fn is_assistant_message(v: &Value) -> bool {
    let has_type = v.get("type").and_then(|x| x.as_str()) == Some("assistant");
    let has_message_role = v["message"]["role"].as_str() == Some("assistant");
    let has_usage = v["message"]["usage"].is_object();
    let has_req_id = v.get("requestId").and_then(|x| x.as_str()).is_some();

    (has_type || has_message_role) && has_usage && has_req_id
}

fn extract_request(v: &Value) -> Option<Request> {
    let req_id = v.get("requestId")?.as_str()?.to_string();
    let message = &v["message"];

    let tools = extract_tools(&message["content"]);

    let usage = &message["usage"];
    let input = usage["input_tokens"].as_i64().unwrap_or(0);
    let output = usage["output_tokens"].as_i64().unwrap_or(0);
    let cache_read = usage["cache_read_input_tokens"].as_i64().unwrap_or(0);
    let cache_create = usage["cache_creation_input_tokens"].as_i64().unwrap_or(0);

    Some(Request {
        req_id,
        tools,
        input,
        output,
        cache_read,
        cache_create,
        delta_cc: 0, // populated later by compute_deltas()
        file_idx: 0, // populated later by parse_file()
    })
}

fn extract_tools(content: &Value) -> Vec<ToolUse> {
    let mut tools = Vec::new();
    if let Some(arr) = content.as_array() {
        for item in arr {
            if item["type"].as_str() == Some("tool_use") {
                if let Some(name) = item["name"].as_str() {
                    tools.push(ToolUse {
                        id: item["id"].as_str().map(str::to_string),
                        name: name.to_string(),
                        input: item.get("input").cloned().unwrap_or(Value::Null),
                    });
                }
            }
        }
    }
    tools
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
) -> Option<(Vec<PathBuf>, Vec<Request>)> {
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

    let requests = parse_requests(&files);
    Some((files, requests))
}

/// cmd_session: analyze session logs (full report).
fn cmd_session(mode: &str, value: &str, project: Option<&str>) -> Value {
    let (files, requests) = match collect_requests(mode, value, project) {
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

    let (startup_costs, per_session_max) = per_session_stats(&files);

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
    let (files, requests) = match collect_requests(mode, value, project) {
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
            let (sc, ms) = per_session_stats(&files);
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
    raw.sort_by(|a, b| b.1.cmp(&a.1));

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
                let query = t
                    .input
                    .get("query")
                    .and_then(|x| x.as_str())
                    .unwrap_or("?")
                    .to_string();
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
        a_cc.cmp(&b_cc)
    });

    json!(items)
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
                if let Some(fp) = tool.input.get("file_path").and_then(|x| x.as_str()) {
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
        a_total.cmp(&b_total)
    });

    items.truncate(10);
    json!(items)
}

/// Detect large responses (output > 10000).
fn detect_large_responses(requests: &[Request]) -> Value {
    let mut items = Vec::new();

    for (idx, r) in requests.iter().enumerate() {
        if r.output > 10000 {
            let tool_key = r.tool_key();
            let file = r.tools.first().and_then(|t| {
                t.input.get("file_path").and_then(|x| {
                    let s = x.as_str().unwrap_or("");
                    let trimmed = if s.len() > 100 { &s[..100] } else { s };
                    Some(trimmed.to_string())
                })
            });

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
        a_output.cmp(&b_output)
    });

    items.truncate(20);
    json!(items)
}

/// Detect large operations (delta_cc > 5000).
fn detect_large_ops(requests: &[Request]) -> Value {
    let mut items = Vec::new();

    for r in requests.iter().filter(|r| r.delta_cc > 5000) {
        let tool_key = r.tool_key();
        let file = r.tools.first().and_then(|t| {
            t.input.get("file_path").and_then(|x| {
                let s = x.as_str().unwrap_or("");
                let trimmed = if s.len() > 100 { &s[..100] } else { s };
                Some(trimmed.to_string())
            })
        });

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
                if let Some(fp) = tool.input.get("file_path").and_then(|x| x.as_str()) {
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
        a_total.cmp(&b_total)
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

/// Per-session stats: first_total_ctx and max_ctx.
fn per_session_stats(files: &[PathBuf]) -> (Value, Value) {
    let mut startup_costs = Vec::new();
    let mut per_session_max = Vec::new();

    for file in files {
        let requests = parse_requests(&[file.clone()]);
        if requests.is_empty() {
            continue;
        }

        let first_total_ctx = requests[0].total_ctx();
        let max_ctx = requests.iter().map(|r| r.total_ctx()).max().unwrap_or(0);

        // Skip invalid data (negative or zero)
        if first_total_ctx <= 0 {
            continue;
        }

        let session_id = file
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

    // Sort descending
    startup_costs.sort_by(|a, b| {
        let a_ft = b["first_total_ctx"].as_i64().unwrap_or(0);
        let b_ft = a["first_total_ctx"].as_i64().unwrap_or(0);
        a_ft.cmp(&b_ft)
    });

    per_session_max.sort_by(|a, b| {
        let a_mx = b["max_ctx"].as_i64().unwrap_or(0);
        let b_mx = a["max_ctx"].as_i64().unwrap_or(0);
        a_mx.cmp(&b_mx)
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

    let reqs1 = parse_requests(&[file1.unwrap().clone()]);
    let reqs2 = parse_requests(&[file2.unwrap().clone()]);

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
    let mut map: HashMap<String, i64> = HashMap::new();
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

        let reqs = parse_requests(&[log]);
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

        let reqs = parse_requests(&[log]);
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

        let reqs = parse_requests(&[f1, f2]);
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

        let reqs = parse_requests(&[log]);
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
}
