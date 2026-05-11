use anyhow::{Context, Result, bail};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub fn run(dot_arg: Option<String>) {
    let dot = match resolve_dotfiles(dot_arg) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("dotcli sync: {e}");
            return;
        }
    };

    let mut synced = false;
    if let Err(e) = sync_claude_settings(&dot) {
        eprintln!("dotcli sync settings: {e}");
    }
    if let Err(e) = sync_claude_keybindings(&dot) {
        eprintln!("dotcli sync keybindings: {e}");
    }
    match sync_scoopfile(&dot) {
        Ok(true) => synced = true,
        Ok(false) => {}
        Err(e) => eprintln!("dotcli sync scoop: {e}"),
    }
    match sync_wingetfile(&dot) {
        Ok(true) => synced = true,
        Ok(false) => {}
        Err(e) => eprintln!("dotcli sync winget: {e}"),
    }
    match sync_mcp_servers(&dot) {
        Ok(true) => synced = true,
        Ok(false) => {}
        Err(e) => eprintln!("dotcli sync mcp: {e}"),
    }
    if synced {
        println!("synced");
    }
}

fn resolve_dotfiles(arg: Option<String>) -> Result<PathBuf> {
    if let Some(p) = arg {
        return Ok(PathBuf::from(p));
    }
    if let Ok(env) = std::env::var("DOTFILES_DIR") {
        if !env.is_empty() {
            return Ok(PathBuf::from(env));
        }
    }
    bail!("DOTFILES_DIR not set; pass --dot <path>")
}

// --- 1) Claude settings.json merge ---

fn sync_claude_settings(dot: &Path) -> Result<()> {
    let template_path = dot.join("claude").join("settings.json");
    if !template_path.exists() {
        return Ok(());
    }
    let template_str = std::fs::read_to_string(&template_path)?;
    let template: Value = serde_json::from_str(&template_str)?;
    let template_obj = template
        .as_object()
        .context("claude/settings.json is not a JSON object")?;

    for dir in claude_dirs()? {
        let target = dir.join("settings.json");
        let mut existing: Map<String, Value> = match std::fs::read_to_string(&target) {
            Ok(s) => match serde_json::from_str::<Value>(&s) {
                Ok(Value::Object(m)) => m,
                _ => Map::new(),
            },
            Err(_) => Map::new(),
        };
        for (k, v) in template_obj {
            existing.insert(k.clone(), v.clone());
        }
        let serialized = serde_json::to_string_pretty(&Value::Object(existing))?;
        write_if_different(&target, &serialized)?;
    }
    Ok(())
}

// --- 1b) Claude keybindings.json 3-way merge ---
//
// 前回 sync 時のテンプレート (.dotcli-keybindings-prev.json) を保存し、
// prev / template / existing の 3-way merge で
// 「テンプレートからの削除」もローカルに伝搬させる。
//
// 各 (context, key) について:
//   - template にあり        : テンプレート値で上書き
//   - prev にあるが template に無い : テンプレートから削除されたとみなしローカルから削除
//   - prev にも template にも無い  : ローカル独自バインドとして保持
//
// prev が存在しない初回 sync 時は削除判定をスキップし、単純マージとして振る舞う。

fn sync_claude_keybindings(dot: &Path) -> Result<()> {
    let template_path = dot.join("claude").join("keybindings.json");
    if !template_path.exists() {
        return Ok(());
    }
    let template_str = std::fs::read_to_string(&template_path)?;
    let template: Value = serde_json::from_str(&template_str)?;
    let template_obj = template
        .as_object()
        .context("claude/keybindings.json is not a JSON object")?;

    for dir in claude_dirs()? {
        let target = dir.join("keybindings.json");
        let prev_path = dir.join(".dotcli-keybindings-prev.json");

        let mut existing: Map<String, Value> = match std::fs::read_to_string(&target) {
            Ok(s) => match serde_json::from_str::<Value>(&s) {
                Ok(Value::Object(m)) => m,
                _ => Map::new(),
            },
            Err(_) => Map::new(),
        };

        let prev: Option<Value> = std::fs::read_to_string(&prev_path)
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok());
        let prev_bindings = prev
            .as_ref()
            .and_then(|v| v.as_object())
            .and_then(|o| o.get("bindings"));

        for (k, v) in template_obj {
            if k == "bindings" {
                let merged =
                    three_way_merge_keybindings_array(prev_bindings, existing.get("bindings"), v);
                existing.insert(k.clone(), merged);
            } else {
                existing.insert(k.clone(), v.clone());
            }
        }
        let serialized = serde_json::to_string_pretty(&Value::Object(existing))?;
        write_if_different(&target, &serialized)?;

        // 今回適用したテンプレートを次回比較用に保存
        std::fs::write(&prev_path, &template_str)?;
    }
    Ok(())
}

fn three_way_merge_keybindings_array(
    prev: Option<&Value>,
    existing: Option<&Value>,
    template: &Value,
) -> Value {
    use std::collections::{BTreeSet, HashMap};

    let template_arr = match template.as_array() {
        Some(a) => a,
        None => return template.clone(),
    };

    // prev: context -> 管理対象キー集合
    let mut prev_keys_by_ctx: HashMap<String, BTreeSet<String>> = HashMap::new();
    if let Some(prev_arr) = prev.and_then(|v| v.as_array()) {
        for entry in prev_arr {
            let Some(obj) = entry.as_object() else { continue };
            let Some(ctx) = obj.get("context").and_then(|v| v.as_str()) else { continue };
            let keys: BTreeSet<String> = obj
                .get("bindings")
                .and_then(|v| v.as_object())
                .map(|m| m.keys().cloned().collect())
                .unwrap_or_default();
            prev_keys_by_ctx.insert(ctx.to_string(), keys);
        }
    }

    // template: context -> bindings & extras（context, bindings 以外のキー）
    let mut tpl_bindings_by_ctx: HashMap<String, Map<String, Value>> = HashMap::new();
    let mut tpl_extras_by_ctx: HashMap<String, Map<String, Value>> = HashMap::new();
    let mut tpl_ctx_order: Vec<String> = Vec::new();
    for entry in template_arr {
        let Some(obj) = entry.as_object() else { continue };
        let Some(ctx) = obj.get("context").and_then(|v| v.as_str()) else { continue };
        let bindings = obj
            .get("bindings")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        tpl_bindings_by_ctx.insert(ctx.to_string(), bindings);
        let mut extras = Map::new();
        for (k, v) in obj {
            if k != "bindings" && k != "context" {
                extras.insert(k.clone(), v.clone());
            }
        }
        tpl_extras_by_ctx.insert(ctx.to_string(), extras);
        if !tpl_ctx_order.contains(&ctx.to_string()) {
            tpl_ctx_order.push(ctx.to_string());
        }
    }

    let mut result: Vec<Value> = match existing.and_then(|v| v.as_array()) {
        Some(a) => a.clone(),
        None => Vec::new(),
    };

    let mut existing_indices: HashMap<String, usize> = HashMap::new();
    for (i, entry) in result.iter().enumerate() {
        if let Some(ctx) = entry
            .as_object()
            .and_then(|o| o.get("context"))
            .and_then(|v| v.as_str())
        {
            existing_indices.insert(ctx.to_string(), i);
        }
    }

    // 関係する context: prev ∪ template
    let mut all_ctxs: BTreeSet<String> = BTreeSet::new();
    all_ctxs.extend(prev_keys_by_ctx.keys().cloned());
    all_ctxs.extend(tpl_bindings_by_ctx.keys().cloned());

    for ctx in &all_ctxs {
        let prev_keys = prev_keys_by_ctx.get(ctx);
        let tpl_bindings = tpl_bindings_by_ctx.get(ctx);
        let tpl_extras = tpl_extras_by_ctx.get(ctx);

        match existing_indices.get(ctx) {
            Some(&i) => {
                let existing_entry = result[i].as_object().cloned().unwrap_or_default();
                let mut bindings_map: Map<String, Value> = existing_entry
                    .get("bindings")
                    .and_then(|v| v.as_object())
                    .cloned()
                    .unwrap_or_default();

                // prev にあって template に無いキーを削除
                if let Some(pk) = prev_keys {
                    for k in pk {
                        let in_tpl = tpl_bindings.map(|m| m.contains_key(k)).unwrap_or(false);
                        if !in_tpl {
                            bindings_map.remove(k);
                        }
                    }
                }

                // template の値で上書き
                if let Some(tpl) = tpl_bindings {
                    for (k, v) in tpl {
                        bindings_map.insert(k.clone(), v.clone());
                    }
                }

                let mut merged = existing_entry;
                merged.insert("bindings".to_string(), Value::Object(bindings_map));
                if let Some(extras) = tpl_extras {
                    for (k, v) in extras {
                        merged.insert(k.clone(), v.clone());
                    }
                }
                result[i] = Value::Object(merged);
            }
            None => {
                // existing に context が無い → template にあれば追加（prev のみは無視）
                if let Some(tpl) = tpl_bindings {
                    let mut new_obj = Map::new();
                    new_obj.insert("context".to_string(), Value::String(ctx.clone()));
                    new_obj.insert("bindings".to_string(), Value::Object(tpl.clone()));
                    if let Some(extras) = tpl_extras {
                        for (k, v) in extras {
                            new_obj.insert(k.clone(), v.clone());
                        }
                    }
                    result.push(Value::Object(new_obj));
                }
            }
        }
    }

    Value::Array(result)
}

fn claude_dirs() -> Result<Vec<PathBuf>> {
    let home = dirs::home_dir().context("home dir not found")?;
    let mut out = Vec::new();
    let primary = home.join(".claude");
    if primary.is_dir() {
        out.push(primary);
    }
    if let Ok(entries) = std::fs::read_dir(&home) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with(".claude-") && e.path().is_dir() {
                out.push(e.path());
            }
        }
    }
    Ok(out)
}

// --- 2) Scoopfile ---

fn sync_scoopfile(dot: &Path) -> Result<bool> {
    if which("scoop").is_none() {
        return Ok(false);
    }
    // scoop is a .cmd / .ps1 shim → invoke via cmd /C for reliability
    let out = Command::new("cmd")
        .args(["/C", "scoop", "export"])
        .stderr(Stdio::null())
        .output();
    let out = match out {
        Ok(o) if o.status.success() => o,
        _ => return Ok(false),
    };
    let export: Value = match serde_json::from_slice(&out.stdout) {
        Ok(v) => v,
        Err(_) => return Ok(false),
    };
    let buckets = filter_keys(export.get("buckets"), &["Name", "Source"]);
    let apps = filter_keys(export.get("apps"), &["Name", "Source"]);

    let mut new_obj = Map::new();
    new_obj.insert("buckets".into(), buckets);
    new_obj.insert("apps".into(), apps);
    let new_str = serde_json::to_string_pretty(&Value::Object(new_obj))?;

    let path = dot.join("install").join("scoopfile.json");
    write_if_different(&path, &new_str)
}

fn filter_keys(value: Option<&Value>, keys: &[&str]) -> Value {
    let Some(items) = value.and_then(|v| v.as_array()) else {
        return Value::Array(Vec::new());
    };
    let filtered: Vec<Value> = items
        .iter()
        .map(|item| {
            if let Some(obj) = item.as_object() {
                let mut out = Map::new();
                for k in keys {
                    if let Some(v) = obj.get(*k) {
                        out.insert((*k).to_string(), v.clone());
                    }
                }
                Value::Object(out)
            } else {
                item.clone()
            }
        })
        .collect();
    Value::Array(filtered)
}

// --- 3) Wingetfile ---

const WINGET_EXCLUDE: &[&str] = &[
    "Amazon.AWSCLI",
    "Oracle.JDK.17",
    "Python.Python.3.12",
    "Python.Python.3.14",
];

fn sync_wingetfile(dot: &Path) -> Result<bool> {
    if which("winget").is_none() {
        return Ok(false);
    }
    let tmp = std::env::temp_dir().join(format!("winget-export-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    let status = Command::new("winget")
        .arg("export")
        .arg("-o")
        .arg(&tmp)
        .arg("--accept-source-agreements")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let result = (|| -> Result<bool> {
        if !matches!(&status, Ok(s) if s.success()) {
            return Ok(false);
        }
        if !tmp.exists() {
            return Ok(false);
        }
        let data = std::fs::read_to_string(&tmp)?;
        if data.trim().is_empty() {
            return Ok(false);
        }
        let export: Value = serde_json::from_str(&data)?;
        let mut apps: Vec<Map<String, Value>> = Vec::new();
        if let Some(sources) = export.get("Sources").and_then(|v| v.as_array()) {
            for src in sources {
                let name = src
                    .get("SourceDetails")
                    .and_then(|d| d.get("Name"))
                    .and_then(|n| n.as_str());
                if name != Some("winget") {
                    continue;
                }
                let Some(packages) = src.get("Packages").and_then(|p| p.as_array()) else {
                    continue;
                };
                for pkg in packages {
                    let Some(id) = pkg.get("PackageIdentifier").and_then(|v| v.as_str()) else {
                        continue;
                    };
                    if WINGET_EXCLUDE.contains(&id) {
                        continue;
                    }
                    let mut m = Map::new();
                    m.insert("Id".into(), Value::String(id.to_string()));
                    apps.push(m);
                }
            }
        }
        apps.sort_by(|a, b| {
            let sa = a.get("Id").and_then(|v| v.as_str()).unwrap_or("");
            let sb = b.get("Id").and_then(|v| v.as_str()).unwrap_or("");
            sa.cmp(sb)
        });
        let apps_value = Value::Array(apps.into_iter().map(Value::Object).collect());
        let mut new_obj = Map::new();
        new_obj.insert("apps".into(), apps_value);
        let new_str = serde_json::to_string_pretty(&Value::Object(new_obj))?;
        let path = dot.join("install").join("wingetfile.json");
        write_if_different(&path, &new_str)
    })();
    let _ = std::fs::remove_file(&tmp);
    result
}

// --- 4) MCP servers placeholder expansion + injection ---

fn sync_mcp_servers(dot: &Path) -> Result<bool> {
    // node availability is not required (Rust handles JSON injection directly)
    let mcp_file = dot.join("claude").join("mcp-servers.json");
    if !mcp_file.exists() {
        return Ok(false);
    }
    let raw = std::fs::read_to_string(&mcp_file)?;

    let home = dirs::home_dir().context("home dir not found")?;
    let projects = dot.parent().unwrap_or(dot).to_path_buf();
    let localappdata = std::env::var("LOCALAPPDATA").unwrap_or_default();
    let appdata = std::env::var("APPDATA").unwrap_or_default();

    let to_fwd = |s: &str| s.replace('\\', "/");
    let replaced = raw
        .replace("{HOME}", &to_fwd(&home.display().to_string()))
        .replace("{PROJECTS}", &to_fwd(&projects.display().to_string()))
        .replace("{LOCALAPPDATA}", &to_fwd(&localappdata))
        .replace("{APPDATA}", &to_fwd(&appdata));

    let servers: Value = serde_json::from_str(&replaced)?;
    let servers_obj = servers
        .as_object()
        .context("mcp-servers.json is not a JSON object")?;

    let mut resolved = Map::new();
    for (name, def) in servers_obj {
        let json = serde_json::to_string(def)?;
        if has_unresolved_placeholder(&json) {
            continue;
        }
        let cmd = def.get("command").and_then(|v| v.as_str()).unwrap_or("");
        if !command_exists(cmd) {
            continue;
        }
        resolved.insert(name.clone(), def.clone());
    }
    if resolved.is_empty() {
        return Ok(false);
    }
    let resolved_value = Value::Object(resolved);

    let mut dirs: Vec<PathBuf> = Vec::new();
    let primary = home.join(".claude");
    if primary.join(".claude.json").exists() {
        dirs.push(primary);
    }
    if let Ok(entries) = std::fs::read_dir(&home) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with(".claude-") && e.path().is_dir() {
                if e.path().join(".claude.json").exists() {
                    dirs.push(e.path());
                }
            }
        }
    }
    if dirs.is_empty() {
        return Ok(false);
    }

    let mut changed = false;
    for d in &dirs {
        let json_path = d.join(".claude.json");
        let raw = match std::fs::read_to_string(&json_path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let mut data: Value = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let current = data.get("mcpServers").cloned().unwrap_or(Value::Null);
        if current != resolved_value {
            if let Some(obj) = data.as_object_mut() {
                obj.insert("mcpServers".into(), resolved_value.clone());
                let serialized = serde_json::to_string_pretty(&data)?;
                if write_if_different(&json_path, &serialized)? {
                    changed = true;
                }
            }
        }
    }
    Ok(changed)
}

// matches PowerShell regex /\{[A-Z_]+\}/
fn has_unresolved_placeholder(s: &str) -> bool {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            let mut j = i + 1;
            let mut count = 0;
            while j < bytes.len() {
                let c = bytes[j];
                if c == b'}' {
                    if count > 0 {
                        return true;
                    }
                    break;
                }
                if !(c.is_ascii_uppercase() || c == b'_') {
                    break;
                }
                count += 1;
                j += 1;
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }
    false
}

// --- helpers ---

fn which(cmd: &str) -> Option<PathBuf> {
    let path_var = std::env::var("PATH").ok()?;
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT;.COM".to_string())
            .split(';')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect()
    } else {
        Vec::new()
    };
    let sep = if cfg!(windows) { ';' } else { ':' };
    for dir in path_var.split(sep) {
        if dir.is_empty() {
            continue;
        }
        let p = Path::new(dir).join(cmd);
        if p.is_file() {
            return Some(p);
        }
        for ext in &exts {
            let p_ext = Path::new(dir).join(format!("{cmd}{ext}"));
            if p_ext.is_file() {
                return Some(p_ext);
            }
        }
    }
    None
}

fn command_exists(cmd: &str) -> bool {
    if cmd.is_empty() {
        return false;
    }
    let p = Path::new(cmd);
    if p.is_absolute() {
        return p.exists();
    }
    which(cmd).is_some()
}

fn write_if_different(path: &Path, new_content: &str) -> Result<bool> {
    let new_trimmed = new_content.trim_end();
    let old = std::fs::read_to_string(path).unwrap_or_default();
    let old_trimmed = old.trim_end();
    if new_trimmed == old_trimmed {
        return Ok(false);
    }
    let mut payload = String::with_capacity(new_trimmed.len() + 1);
    payload.push_str(new_trimmed);
    payload.push('\n');
    std::fs::write(path, payload)?;
    Ok(true)
}
