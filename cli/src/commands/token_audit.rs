//! Token consumption audit. Replaces bin/token-audit (bash + jq).
//! Direct std::fs usage (no process spawn) for performance on Windows.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// Assistant message extracted from JSONL line. Equivalent to bash JQ_DEDUP.
#[derive(Debug, Clone)]
struct Request {
    req_id: String,
    tools: Vec<ToolUse>,
    input: i64,
    output: i64,
    cache_read: i64,
    cache_create: i64,
    /// Delta from previous request's cache_create (populated after parse).
    delta_cc: i64,
    /// Index of the source file (for per-session delta computation).
    file_idx: usize,
}

#[derive(Debug, Clone)]
struct ToolUse {
    name: String,
    input: Value, // .input field (for file_path extraction)
}

impl Request {
    fn total_ctx(&self) -> i64 {
        self.input + self.cache_read + self.cache_create
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

/// Find all *.jsonl files under ~/.claude/projects and ~/.claude-*/projects.
/// Deduplicate by basename (symlink handling).
/// Includes subagents/ directory.
fn find_jsonl_files() -> Vec<PathBuf> {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return Vec::new(),
    };

    let mut claude_dirs: Vec<PathBuf> = Vec::new();
    if home.join(".claude").exists() {
        claude_dirs.push(home.join(".claude"));
    }

    // Find .claude-* directories
    if let Ok(entries) = fs::read_dir(&home) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name() {
                    let name_str = name.to_string_lossy();
                    if name_str.starts_with(".claude-") {
                        claude_dirs.push(path);
                    }
                }
            }
        }
    }

    let mut files: Vec<PathBuf> = Vec::new();
    for claude_dir in claude_dirs {
        let projects_dir = claude_dir.join("projects");
        if projects_dir.exists() {
            collect_jsonl(&projects_dir, &mut files);
        }
    }

    // Deduplicate by basename (first wins)
    let mut seen: HashMap<String, PathBuf> = HashMap::new();
    for f in files {
        if let Some(name) = f.file_name() {
            let name_str = name.to_string_lossy().into_owned();
            if !seen.contains_key(&name_str) {
                seen.insert(name_str, f);
            }
        }
    }

    seen.into_values().collect()
}

/// Recursively collect *.jsonl files.
fn collect_jsonl(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl(&path, out);
        } else if path.extension().map_or(false, |e| e == "jsonl") {
            out.push(path);
        }
    }
}

/// --last N: sort by mtime descending, take top N.
fn find_latest_files(files: &[PathBuf], n: usize) -> Vec<PathBuf> {
    let mut with_mtime: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();

    for f in files {
        if let Ok(meta) = fs::metadata(f) {
            if let Ok(mtime) = meta.modified() {
                with_mtime.push((mtime, f.clone()));
            }
        }
    }

    with_mtime.sort_by(|a, b| b.0.cmp(&a.0)); // descending
    with_mtime.truncate(n);
    with_mtime.into_iter().map(|(_, p)| p).collect()
}

/// --session ID: filter files containing ID in filename.
fn find_session_files(files: &[PathBuf], id: &str) -> Vec<PathBuf> {
    files
        .iter()
        .filter(|f| {
            f.file_name()
                .and_then(|n| n.to_str())
                .map_or(false, |s| s.contains(id))
        })
        .cloned()
        .collect()
}

// ============================================================================
// JSONL parsing
// ============================================================================

/// Parse JSONL files, extract assistant messages, dedup by requestId.
/// Preserves first-appearance order; last entry wins for duplicate IDs.
/// Equivalent to: jq -c "$JQ_DEDUP" | jq -s "$JQ_DEDUP_GROUP"
fn parse_requests(files: &[PathBuf]) -> Vec<Request> {
    let mut requests: Vec<Request> = Vec::new();
    let mut id_index: HashMap<String, usize> = HashMap::new();

    for (file_idx, file_path) in files.iter().enumerate() {
        if let Ok(f) = fs::File::open(file_path) {
            let reader = BufReader::new(f);
            for line in reader.lines().flatten() {
                if let Ok(val) = serde_json::from_str::<Value>(&line) {
                    if is_assistant_message(&val) {
                        if let Some(mut req) = extract_request(&val) {
                            req.file_idx = file_idx;
                            if let Some(&idx) = id_index.get(&req.req_id) {
                                requests[idx] = req; // last wins, same position
                            } else {
                                id_index.insert(req.req_id.clone(), requests.len());
                                requests.push(req);
                            }
                        }
                    }
                }
            }
        }
    }

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
        file_idx: 0, // populated later by parse_requests()
    })
}

fn extract_tools(content: &Value) -> Vec<ToolUse> {
    let mut tools = Vec::new();
    if let Some(arr) = content.as_array() {
        for item in arr {
            if item["type"].as_str() == Some("tool_use") {
                if let Some(name) = item["name"].as_str() {
                    tools.push(ToolUse {
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

    // .claudeignore
    let ignore_exists = std::env::current_dir()
        .map(|p| p.join(".claudeignore").exists())
        .unwrap_or(false);

    json!({
        "claude_md": claude_md,
        "skills": skills,
        "skills_total_est_tokens": skills_total_est_tokens,
        "plugins": plugins,
        "plugin_count": plugins.len() as i64,
        "mcp_server_count": mcp_server_count,
        "token_audit_hook_active": token_audit_hook_active,
        "claudeignore_exists": ignore_exists
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

/// cmd_session: analyze session logs.
fn cmd_session(mode: &str, value: &str) -> Value {
    let all_files = find_jsonl_files();
    let files = match mode {
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
    };

    if files.is_empty() {
        return json!({"error": "no sessions found"});
    }

    let session_count = files.len();
    let mut requests = parse_requests(&files);
    compute_deltas(&mut requests);
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

/// Aggregate delta_cc and output by tool combination key.
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

    let mut items: Vec<_> = by_tool
        .into_iter()
        .map(|(tool, (delta_cc, total_output))| {
            let pct = if total > 0 {
                (delta_cc as f64 / total as f64) * 100.0
            } else {
                0.0
            };
            let efficiency = if delta_cc > 0 {
                (total_output as f64 / delta_cc as f64 * 10.0).round() / 10.0
            } else {
                0.0
            };
            json!({
                "tool": tool,
                "cache_create": delta_cc,
                "total_output": total_output,
                "pct": (pct * 10.0).round() / 10.0, // 1 decimal
                "efficiency": efficiency
            })
        })
        .collect();

    // Sort by cache_create descending
    items.sort_by(|a, b| {
        let a_cc = b["cache_create"].as_i64().unwrap_or(0);
        let b_cc = a["cache_create"].as_i64().unwrap_or(0);
        a_cc.cmp(&b_cc)
    });

    json!(items)
}

/// Breakdown of ToolSearch queries by delta_cc cost.
fn aggregate_toolsearch(requests: &[Request]) -> Value {
    let mut by_query: HashMap<String, (i64, i64)> = HashMap::new(); // query -> (delta_cc, count)

    for r in requests {
        for t in &r.tools {
            if t.name == "ToolSearch" {
                let query = t
                    .input
                    .get("query")
                    .and_then(|x| x.as_str())
                    .unwrap_or("?")
                    .to_string();
                let entry = by_query.entry(query).or_default();
                entry.0 += r.delta_cc;
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

    let file1 = files1.first();
    let file2 = files2.first();

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
            let (mode, value) = parse_session_args(&args[1..]);
            let result = cmd_session(&mode, &value);
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
            let (mode, value) = if args.len() > 1 {
                parse_session_args(&args[1..])
            } else {
                ("last".to_string(), "10".to_string())
            };
            let static_result = cmd_static();
            let session_result = cmd_session(&mode, &value);
            print!("{}", super::token_audit_format::format_static(&static_result));
            println!();
            print!("{}", super::token_audit_format::format_session(&session_result));
        }
        _ => {
            eprintln!("Usage: dotcli token-audit <static|session|all|compare>");
            eprintln!("  dotcli token-audit static");
            eprintln!("  dotcli token-audit session [--all|--last N|--session ID]");
            eprintln!("  dotcli token-audit all [--last N|--session ID]  (default: --last 10)");
            eprintln!("  dotcli token-audit compare <id1> <id2>");
            std::process::exit(1);
        }
    }
}

fn parse_session_args(args: &[String]) -> (String, String) {
    let mut mode = "all".to_string();
    let mut value = String::new();
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
            _ => {
                eprintln!("Unknown session option: {}", args[i]);
                std::process::exit(1);
            }
        }
    }
    (mode, value)
}
