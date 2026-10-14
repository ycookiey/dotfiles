use anyhow::{Context, Result, bail};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

/// Minimum time between two scoop / winget / VS Code exports. Shells start `dotcli sync`
/// in the background on every launch; the exports spawn scoop (PowerShell), winget and
/// code and take seconds, so they are throttled while the cheap merges (settings,
/// keybindings, MCP) run every time. `--force` bypasses the throttle.
const EXPORT_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

pub fn run(dot_arg: Option<String>, force: bool) {
    let dot = match resolve_dotfiles(dot_arg) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("dotcli sync: {e}");
            return;
        }
    };

    let mut synced = false;
    let home = dirs::home_dir();
    let claude_dirs = match &home {
        Some(h) => claude_dirs(h),
        None => {
            eprintln!("dotcli sync: home dir not found");
            Vec::new()
        }
    };
    if let Err(e) = sync_claude_settings(&dot, &claude_dirs) {
        eprintln!("dotcli sync settings: {e}");
    }
    if let Err(e) = sync_claude_keybindings(&dot, &claude_dirs) {
        eprintln!("dotcli sync keybindings: {e}");
    }
    let stamp_dir = stamp_dir();
    let now = SystemTime::now();
    match sync_scoopfile(&dot, stamp_dir.as_deref(), force, now) {
        Ok(true) => synced = true,
        Ok(false) => {}
        Err(e) => eprintln!("dotcli sync scoop: {e}"),
    }
    match sync_wingetfile(&dot, stamp_dir.as_deref(), force, now) {
        Ok(true) => synced = true,
        Ok(false) => {}
        Err(e) => eprintln!("dotcli sync winget: {e}"),
    }
    match sync_vscode(&dot, stamp_dir.as_deref(), force, now) {
        Ok(true) => synced = true,
        Ok(false) => {}
        Err(e) => eprintln!("dotcli sync vscode: {e}"),
    }
    if let Some(home) = &home {
        match sync_mcp_servers(&dot, home, &claude_dirs) {
            Ok(true) => synced = true,
            Ok(false) => {}
            Err(e) => eprintln!("dotcli sync mcp: {e}"),
        }
    }
    if synced {
        println!("synced");
    }
}

fn resolve_dotfiles(arg: Option<String>) -> Result<PathBuf> {
    if let Some(p) = arg {
        return Ok(PathBuf::from(p));
    }
    match crate::paths::dotfiles_root() {
        Some(p) => Ok(p),
        None => bail!("dotfiles root not found (DOTFILES_DIR unset); pass --dot <path>"),
    }
}

// --- 1) Claude settings.json merge ---

fn sync_claude_settings(dot: &Path, claude_dirs: &[PathBuf]) -> Result<()> {
    let template_path = dot.join("claude").join("settings.json");
    if !template_path.exists() {
        return Ok(());
    }
    let template_str = std::fs::read_to_string(&template_path)?;
    let template: Value = serde_json::from_str(&template_str)?;
    let template_obj = template
        .as_object()
        .context("claude/settings.json is not a JSON object")?;

    // `.claude-<n>/settings.json` is usually a symlink to `~/.claude`'s: merge each file once.
    for target in unique_files(claude_dirs.iter().map(|d| d.join("settings.json"))) {
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

fn sync_claude_keybindings(dot: &Path, claude_dirs: &[PathBuf]) -> Result<()> {
    let template_path = dot.join("claude").join("keybindings.json");
    if !template_path.exists() {
        return Ok(());
    }
    let template_str = std::fs::read_to_string(&template_path)?;
    let template: Value = serde_json::from_str(&template_str)?;
    let template_obj = template
        .as_object()
        .context("claude/keybindings.json is not a JSON object")?;

    // The prev file of every dir is read before any is rewritten: in the usual setup the
    // `.claude-<n>` prev files are symlinks to `~/.claude`'s, so updating it first would
    // make the later dirs see prev == template and keep bindings the template dropped.
    // Snapshots are keyed by the resolved file, so separate prev files also work.
    let prev_path = |dir: &Path| dir.join(".dotcli-keybindings-prev.json");
    let mut prev_snapshots: HashMap<PathBuf, Option<Value>> = HashMap::new();
    for dir in claude_dirs {
        let p = prev_path(dir);
        prev_snapshots.entry(file_key(&p)).or_insert_with(|| {
            std::fs::read_to_string(&p)
                .ok()
                .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        });
    }

    let mut seen_targets = HashSet::new();
    for dir in claude_dirs {
        let target = dir.join("keybindings.json");
        if !seen_targets.insert(file_key(&target)) {
            continue;
        }

        let mut existing: Map<String, Value> = match std::fs::read_to_string(&target) {
            Ok(s) => match serde_json::from_str::<Value>(&s) {
                Ok(Value::Object(m)) => m,
                _ => Map::new(),
            },
            Err(_) => Map::new(),
        };

        let prev = prev_snapshots
            .get(&file_key(&prev_path(dir)))
            .and_then(|v| v.as_ref());
        let prev_bindings = prev
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
    }

    // 今回適用したテンプレートを次回比較用に保存 (全 keybindings.json の更新後、実体ごとに 1 回)
    for p in unique_files(claude_dirs.iter().map(|d| prev_path(d))) {
        write_if_different(&p, &template_str)?;
    }
    Ok(())
}

fn three_way_merge_keybindings_array(
    prev: Option<&Value>,
    existing: Option<&Value>,
    template: &Value,
) -> Value {
    use std::collections::BTreeSet;

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

/// Claude config dirs in `home`: `~/.claude` first, then the account dirs
/// (`is_account_dir_name`) in name order. Read once per sync and shared by every step.
fn claude_dirs(home: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let primary = home.join(".claude");
    if primary.is_dir() {
        out.push(primary);
    }
    let mut accounts: Vec<PathBuf> = std::fs::read_dir(home)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| is_account_dir_name(&e.file_name().to_string_lossy()) && e.path().is_dir())
        .map(|e| e.path())
        .collect();
    accounts.sort();
    out.extend(accounts);
    out
}

/// Extra Claude account config dirs are named `.claude-<digits>` (`^\.claude-\d+$`).
/// Other `.claude-*` dirs in home (e.g. `.claude-browsers`, `.claude-1.lock`) are not
/// accounts and must not receive settings / keybindings / MCP config.
fn is_account_dir_name(name: &str) -> bool {
    name.strip_prefix(".claude-")
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

// --- 2a) Export throttle (scoop / winget / vscode) ---
//
// One stamp file per export kind: `<cache_dir>/dotcli/sync-<kind>.stamp`
// (%LOCALAPPDATA%\dotcli on Windows). Its mtime is when the last export that went on
// to succeed started. The stamp is claimed (rewritten) *before* the export runs, so N
// shells starting together see a fresh stamp after the first one and skip, instead of
// all running the export side by side; if the export fails, the claim is rolled back
// to the previous mtime (or the stamp removed) so the next start retries.
//
// The stamp is global, not per dotfiles dir: a sync run against another checkout also
// defers the next export of this one by up to EXPORT_INTERVAL.

fn stamp_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("dotcli"))
}

struct ExportClaim {
    stamp: Option<PathBuf>,
    prev: Option<SystemTime>,
}

impl ExportClaim {
    /// Returns None when the export of `kind` ran within EXPORT_INTERVAL before `now`
    /// and `force` is off. Otherwise claims the stamp and returns the claim. A stamp
    /// that cannot be written does not block the export (it just isn't throttled).
    fn acquire(dir: Option<&Path>, kind: &str, force: bool, now: SystemTime) -> Option<Self> {
        let Some(dir) = dir else {
            return Some(Self { stamp: None, prev: None });
        };
        let stamp = dir.join(format!("sync-{kind}.stamp"));
        let prev = std::fs::metadata(&stamp).and_then(|m| m.modified()).ok();
        if !force && prev.is_some_and(|t| is_fresh(t, now)) {
            return None;
        }
        let written = std::fs::create_dir_all(dir).and_then(|()| touch(&stamp, now));
        Some(Self {
            stamp: written.ok().map(|()| stamp),
            prev,
        })
    }

    /// Undo the claim after a failed export so the next run does not skip it.
    fn release(self) {
        let Some(stamp) = self.stamp else { return };
        let restored = match self.prev {
            Some(t) => std::fs::File::options()
                .write(true)
                .open(&stamp)
                .and_then(|f| f.set_modified(t)),
            None => std::fs::remove_file(&stamp),
        };
        if restored.is_err() {
            let _ = std::fs::remove_file(&stamp);
        }
    }
}

/// A stamp in the future (clock moved back) counts as stale.
fn is_fresh(stamp: SystemTime, now: SystemTime) -> bool {
    now.duration_since(stamp)
        .is_ok_and(|elapsed| elapsed < EXPORT_INTERVAL)
}

fn touch(path: &Path, now: SystemTime) -> std::io::Result<()> {
    let f = std::fs::File::create(path)?;
    f.set_modified(now)
}

/// Runs `export` unless throttled. `export` returns `Ok(None)` when the export did not
/// produce usable output (command failed, unparsable output), `Ok(Some(changed))` when
/// it did; only the latter keeps the stamp claimed.
fn run_throttled(
    dir: Option<&Path>,
    kind: &str,
    force: bool,
    now: SystemTime,
    export: impl FnOnce() -> Result<Option<bool>>,
) -> Result<bool> {
    let Some(claim) = ExportClaim::acquire(dir, kind, force, now) else {
        return Ok(false);
    };
    let result = export();
    if !matches!(result, Ok(Some(_))) {
        claim.release();
    }
    Ok(result?.unwrap_or(false))
}

// --- 2) Scoopfile ---

fn sync_scoopfile(
    dot: &Path,
    stamp_dir: Option<&Path>,
    force: bool,
    now: SystemTime,
) -> Result<bool> {
    if which("scoop").is_none() {
        return Ok(false);
    }
    run_throttled(stamp_dir, "scoop", force, now, || export_scoopfile(dot))
}

fn export_scoopfile(dot: &Path) -> Result<Option<bool>> {
    // scoop is a .cmd / .ps1 shim → invoke via cmd /C for reliability
    let out = Command::new("cmd")
        .args(["/C", "scoop", "export"])
        .stderr(Stdio::null())
        .output();
    let out = match out {
        Ok(o) if o.status.success() => o,
        _ => return Ok(None),
    };
    let export: Value = match serde_json::from_slice(&out.stdout) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    let buckets = filter_keys(export.get("buckets"), &["Name", "Source"]);
    let apps = filter_keys(export.get("apps"), &["Name", "Source"]);

    let mut new_obj = Map::new();
    new_obj.insert("buckets".into(), buckets);
    new_obj.insert("apps".into(), apps);
    let new_str = serde_json::to_string_pretty(&Value::Object(new_obj))?;

    let path = dot.join("install").join("scoopfile.json");
    write_if_different(&path, &new_str).map(Some)
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

/// winget IDs left out of wingetfile.json, compared without regard to case (winget IDs
/// are case-insensitive). A rule ending in `.` matches every ID that starts with it;
/// any other rule matches one ID exactly. Keep prefixes ending in `.` so that, for
/// example, `Microsoft.Edge` does not also drop `Microsoft.Edge.Beta`.
const WINGET_EXCLUDE: &[&str] = &[
    // Installed by other means.
    "Amazon.AWSCLI",
    "Oracle.JDK.17",
    "Python.Python.3.12",
    "Python.Python.3.14",
    // Runtimes that the apps needing them install themselves.
    "Microsoft.VCRedist.",
    "Microsoft.VCLibs.",
    "Microsoft.UI.Xaml.",
    "Microsoft.WindowsAppRuntime.",
    "Microsoft.DotNet.Native.Runtime",
    "EpicGames.EpicOnlineServices",
    "Nvidia.PhysX",
    // Shipped with Windows.
    "Microsoft.AppInstaller",
    "Microsoft.Edge",
    "Microsoft.OneDrive",
    // Set up by install/wsl.ps1.
    "Microsoft.WSL",
    "Canonical.Ubuntu",
    // VS Build Tools come from Microsoft.VisualStudio.BuildTools, which winget.ps1
    // installs with the C++ workload; an older line would install without workloads.
    "Microsoft.VisualStudio.2019.BuildTools",
];

/// IDs kept although a rule in WINGET_EXCLUDE matches them. The 2015+ x64 runtime is
/// the one an app installed without its own redistributable is most likely to need.
const WINGET_KEEP: &[&str] = &["Microsoft.VCRedist.2015+.x64"];

/// winget ID → scoop app name for apps both managers can install. When the scoop name
/// is in scoopfile.json the app is left to scoop and the winget ID is dropped.
const WINGET_SCOOP_DUPLICATES: &[(&str, &str)] = &[
    ("AutoHotkey.AutoHotkey", "autohotkey"),
    ("Discord.Discord", "discord"),
    ("EpicGames.EpicGamesLauncher", "epic-games-launcher"),
    ("junegunn.fzf", "fzf"),
    ("Parsec.Parsec", "parsec-np"),
    ("Zoom.Zoom.EXE", "zoom"),
];

/// Whether `id` stays out of wingetfile.json. `scoop_apps` holds the lowercased app
/// names from scoopfile.json.
fn winget_excluded(id: &str, scoop_apps: &HashSet<String>) -> bool {
    let lower = id.to_ascii_lowercase();
    if WINGET_KEEP.iter().any(|k| k.eq_ignore_ascii_case(id)) {
        return false;
    }
    let ruled_out = WINGET_EXCLUDE.iter().any(|rule| {
        let rule = rule.to_ascii_lowercase();
        if rule.ends_with('.') {
            lower.starts_with(&rule)
        } else {
            lower == rule
        }
    });
    ruled_out
        || WINGET_SCOOP_DUPLICATES.iter().any(|(winget, scoop)| {
            winget.eq_ignore_ascii_case(id) && scoop_apps.contains(*scoop)
        })
}

/// Lowercased app names in install/scoopfile.json; empty when it is missing or
/// unreadable, so no winget ID is then dropped as a scoop duplicate.
fn scoop_app_names(dot: &Path) -> HashSet<String> {
    let path = dot.join("install").join("scoopfile.json");
    let Ok(data) = std::fs::read_to_string(&path) else {
        return HashSet::new();
    };
    let Ok(v) = serde_json::from_str::<Value>(&data) else {
        return HashSet::new();
    };
    v.get("apps")
        .and_then(|a| a.as_array())
        .map(|apps| {
            apps.iter()
                .filter_map(|app| app.get("Name").and_then(|n| n.as_str()))
                .map(|n| n.to_ascii_lowercase())
                .collect()
        })
        .unwrap_or_default()
}

/// wingetfile.json content for a `winget export` document: the winget-source package
/// IDs that `winget_excluded` keeps, sorted.
fn build_wingetfile(export: &Value, scoop_apps: &HashSet<String>) -> Result<String> {
    let mut ids: Vec<&str> = Vec::new();
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
                if !winget_excluded(id, scoop_apps) {
                    ids.push(id);
                }
            }
        }
    }
    ids.sort_unstable();
    let apps: Vec<Value> = ids
        .into_iter()
        .map(|id| {
            let mut m = Map::new();
            m.insert("Id".into(), Value::String(id.to_string()));
            Value::Object(m)
        })
        .collect();
    let mut new_obj = Map::new();
    new_obj.insert("apps".into(), Value::Array(apps));
    Ok(serde_json::to_string_pretty(&Value::Object(new_obj))?)
}

fn sync_wingetfile(
    dot: &Path,
    stamp_dir: Option<&Path>,
    force: bool,
    now: SystemTime,
) -> Result<bool> {
    if which("winget").is_none() {
        return Ok(false);
    }
    run_throttled(stamp_dir, "winget", force, now, || export_wingetfile(dot))
}

fn export_wingetfile(dot: &Path) -> Result<Option<bool>> {
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
    let result = (|| -> Result<Option<bool>> {
        if !matches!(&status, Ok(s) if s.success()) {
            return Ok(None);
        }
        if !tmp.exists() {
            return Ok(None);
        }
        let data = std::fs::read_to_string(&tmp)?;
        if data.trim().is_empty() {
            return Ok(None);
        }
        let export: Value = serde_json::from_str(&data)?;
        let new_str = build_wingetfile(&export, &scoop_app_names(dot))?;
        let path = dot.join("install").join("wingetfile.json");
        write_if_different(&path, &new_str).map(Some)
    })();
    let _ = std::fs::remove_file(&tmp);
    result
}

// --- 3b) VS Code snapshot ---
//
// `<dot>/vscode/` holds a read-only copy of the VS Code extension list, settings.json
// and keybindings.json for sharing. VS Code Settings Sync stays the source of truth:
// nothing here is linked or written back to the live profile. Secrets are replaced
// with REDACTED before writing (see `redact_secrets`); a file that does not parse is
// skipped rather than copied as is. vscode/README.md is maintained by hand.

const REDACTED: &str = "<redacted>";

/// Substrings (compared lowercased) of a key name whose value is redacted, at any depth.
const SECRET_KEY_WORDS: &[&str] = &[
    "token",
    "secret",
    "password",
    "passwd",
    "apikey",
    "api_key",
    "api-key",
    "accesskey",
    "access_key",
    "credential",
    "auth",
    "cookie",
    "session",
    "private",
];

/// Prefixes of well-known token formats, with the minimum length of the rest.
const TOKEN_PREFIXES: &[(&str, usize)] = &[
    ("ghp_", 16),
    ("gho_", 16),
    ("ghu_", 16),
    ("ghs_", 16),
    ("ghr_", 16),
    ("github_pat_", 16),
    ("glpat-", 16),
    ("xoxb-", 10),
    ("xoxp-", 10),
    ("xoxs-", 10),
    ("xoxa-", 10),
    ("AIza", 30),
];

fn vscode_user_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("Code").join("User"))
}

fn sync_vscode(
    dot: &Path,
    stamp_dir: Option<&Path>,
    force: bool,
    now: SystemTime,
) -> Result<bool> {
    let has_code = which("code").is_some();
    let user_dir = vscode_user_dir().filter(|d| d.is_dir());
    if !has_code && user_dir.is_none() {
        return Ok(false);
    }
    run_throttled(stamp_dir, "vscode", force, now, || {
        let extensions = if has_code { list_vscode_extensions() } else { None };
        let export = export_vscode(
            &dot.join("vscode"),
            extensions.as_deref(),
            user_dir.as_deref(),
        )?;
        if !export.redacted.is_empty() {
            // Key paths only; the values never leave this process.
            eprintln!(
                "dotcli sync vscode: redacted {} value(s): {}",
                export.redacted.len(),
                export.redacted.join(", ")
            );
        }
        Ok(export.produced.then_some(export.changed))
    })
}

/// stdout of `code --list-extensions`, or None when it fails.
fn list_vscode_extensions() -> Option<String> {
    // code is a .cmd launcher → invoke via cmd /C like scoop
    let out = Command::new("cmd")
        .args(["/C", "code", "--list-extensions"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

struct VscodeExport {
    /// At least one file was generated (whether or not it differed from the old one).
    produced: bool,
    /// At least one file was rewritten.
    changed: bool,
    /// `<file>#<JSON pointer>` of every redacted value.
    redacted: Vec<String>,
}

/// Writes extensions.txt (from `extensions_stdout`, the output of
/// `code --list-extensions`) and the redacted settings.json / keybindings.json found in
/// `user_dir` into `out_dir`. A missing input leaves the matching file untouched.
fn export_vscode(
    out_dir: &Path,
    extensions_stdout: Option<&str>,
    user_dir: Option<&Path>,
) -> Result<VscodeExport> {
    let mut export = VscodeExport {
        produced: false,
        changed: false,
        redacted: Vec::new(),
    };
    let mut outputs: Vec<(&str, String)> = Vec::new();
    if let Some(list) = extensions_stdout.map(normalize_extensions)
        && !list.is_empty()
    {
        outputs.push(("extensions.txt", list));
    }
    for name in ["settings.json", "keybindings.json"] {
        let Some(src) = user_dir.and_then(|d| std::fs::read_to_string(d.join(name)).ok()) else {
            continue;
        };
        let mut value = match parse_jsonc(&src) {
            Ok(v) => v,
            Err(e) => {
                // Line/column only: never echo the source, it may hold secrets.
                eprintln!(
                    "dotcli sync vscode: {name} skipped (not JSONC, line {} column {})",
                    e.line(),
                    e.column()
                );
                continue;
            }
        };
        for path in redact_secrets(&mut value) {
            export.redacted.push(format!("{name}#{path}"));
        }
        outputs.push((name, serde_json::to_string_pretty(&value)?));
    }
    if outputs.is_empty() {
        return Ok(export);
    }
    std::fs::create_dir_all(out_dir)?;
    for (name, content) in outputs {
        export.produced = true;
        export.changed |= write_if_different(&out_dir.join(name), &content)?;
    }
    Ok(export)
}

/// Extension IDs from `code --list-extensions`, lowercased, sorted, deduplicated, one per
/// line. Lines that are not `publisher.name` (blank, messages) are dropped.
fn normalize_extensions(stdout: &str) -> String {
    let ids: std::collections::BTreeSet<String> = stdout
        .lines()
        .map(str::trim)
        .filter(|l| l.contains('.') && !l.contains(char::is_whitespace))
        .map(str::to_ascii_lowercase)
        .collect();
    ids.into_iter().collect::<Vec<_>>().join("\n")
}

/// Parses JSON with comments (`//`, `/* */`), trailing commas and a BOM, as VS Code
/// accepts in its settings files. Comments are lost. Duplicate keys: the last one wins.
fn parse_jsonc(src: &str) -> serde_json::Result<Value> {
    serde_json::from_str(&strip_trailing_commas(&strip_jsonc_comments(src)))
}

fn strip_jsonc_comments(src: &str) -> String {
    let src = src.strip_prefix('\u{feff}').unwrap_or(src);
    let mut out = String::with_capacity(src.len());
    let mut chars = src.chars().peekable();
    let mut in_str = false;
    while let Some(c) = chars.next() {
        if in_str {
            out.push(c);
            if c == '\\' {
                if let Some(n) = chars.next() {
                    out.push(n);
                }
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_str = true;
                out.push(c);
            }
            '/' if chars.peek() == Some(&'/') => {
                while chars.peek().is_some_and(|&n| n != '\n') {
                    chars.next();
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut prev = '\0';
                for n in chars.by_ref() {
                    if prev == '*' && n == '/' {
                        break;
                    }
                    prev = n;
                }
                out.push(' ');
            }
            _ => out.push(c),
        }
    }
    out
}

/// Drops a `,` that only whitespace separates from the next `}` or `]` (comments must
/// be stripped first).
fn strip_trailing_commas(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut in_str = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if in_str {
            out.push(b);
            if b == b'\\' && i + 1 < bytes.len() {
                out.push(bytes[i + 1]);
                i += 1;
            } else if b == b'"' {
                in_str = false;
            }
        } else if b == b'"' {
            in_str = true;
            out.push(b);
        } else if b == b',' {
            let next = bytes[i + 1..].iter().find(|c| !c.is_ascii_whitespace());
            if !matches!(next, Some(b'}' | b']')) {
                out.push(b);
            }
        } else {
            out.push(b);
        }
        i += 1;
    }
    // Only ASCII bytes were dropped, so the rest is still valid UTF-8.
    String::from_utf8(out).expect("valid UTF-8")
}

/// Replaces secrets in `value` with REDACTED and returns the JSON pointers of the
/// replaced values.
/// (a) A key whose name contains a SECRET_KEY_WORDS entry: its value as a whole (an
///     object or array included), unless it is a bool, a number, null or "".
/// (b) A string that holds a token (`string_has_secret`), wherever it is.
fn redact_secrets(value: &mut Value) -> Vec<String> {
    let mut paths = Vec::new();
    redact_at(value, "", &mut paths);
    paths
}

fn redact_at(value: &mut Value, path: &str, paths: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                let p = format!("{path}/{}", k.replace('~', "~0").replace('/', "~1"));
                let harmless = match v {
                    Value::Bool(_) | Value::Number(_) | Value::Null => true,
                    Value::String(s) => s.is_empty(),
                    _ => false,
                };
                if is_secret_key(k) && !harmless {
                    *v = Value::String(REDACTED.into());
                    paths.push(p);
                } else {
                    redact_at(v, &p, paths);
                }
            }
        }
        Value::Array(items) => {
            for (i, v) in items.iter_mut().enumerate() {
                redact_at(v, &format!("{path}/{i}"), paths);
            }
        }
        Value::String(s) => {
            if string_has_secret(s) {
                *value = Value::String(REDACTED.into());
                paths.push(path.to_string());
            }
        }
        _ => {}
    }
}

fn is_secret_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    SECRET_KEY_WORDS.iter().any(|w| lower.contains(w))
}

/// Whether `s` holds a token: a known token format anywhere in it, a URL with
/// credentials (userinfo, or a secret-named query parameter), or an opaque string
/// (`is_opaque_token`) that is not a path or URL.
fn string_has_secret(s: &str) -> bool {
    s.split_whitespace().any(|piece| {
        if let Some(i) = piece.find("://") {
            return url_has_secret(&piece[i + 3..]);
        }
        piece
            .split(|c: char| "\"'`,;=()<>[]{}".contains(c))
            .any(|w| is_known_token(w) || (!is_path_like(w) && is_opaque_token(w)))
    })
}

/// `rest` is a URL after `scheme://`. URLs are exempt from the opaque-string rule, but
/// not from userinfo, secret-named query parameters and known token formats.
fn url_has_secret(rest: &str) -> bool {
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    if rest[..authority_end].contains('@') {
        return true;
    }
    let (path, query) = match rest.find(['?', '#']) {
        Some(i) => (&rest[..i], &rest[i + 1..]),
        None => (rest, ""),
    };
    if path.split('/').any(is_known_token) {
        return true;
    }
    query.split(['&', '#', ';']).any(|param| {
        let (k, v) = param.split_once('=').unwrap_or((param, ""));
        let k = k.to_ascii_lowercase();
        let secret_name = is_secret_key(&k)
            || matches!(k.as_str(), "key" | "sig" | "signature" | "code" | "pass" | "pwd");
        (secret_name && !v.is_empty()) || is_known_token(v)
    })
}

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || "-_=+/.".contains(c)
}

fn is_known_token(w: &str) -> bool {
    if w.is_empty() || !w.chars().all(is_token_char) {
        return false;
    }
    let prefixed = TOKEN_PREFIXES
        .iter()
        .any(|(p, min)| w.strip_prefix(p).is_some_and(|rest| rest.len() >= *min));
    // OpenAI-style `sk-…`: the rest must mix in digits so `sk-SK` and words stay.
    let openai = w
        .strip_prefix("sk-")
        .is_some_and(|rest| rest.len() >= 20 && rest.chars().any(|c| c.is_ascii_digit()));
    // AWS access key ID: AKIA + 16 of [0-9A-Z].
    let aws = w.match_indices("AKIA").any(|(i, _)| {
        let tail = &w.as_bytes()[i + 4..];
        tail.len() >= 16
            && tail[..16]
                .iter()
                .all(|b| b.is_ascii_digit() || b.is_ascii_uppercase())
    });
    // JWT: base64 of `{"` starts with eyJ.
    let jwt = w.starts_with("eyJ") && w.len() >= 20;
    prefixed || openai || aws || jwt
}

/// Long strings of token characters whose letters and digits alternate the way random
/// strings do. Identifiers such as `ms-vscode-remote.remote-ssh-edit` or
/// `workbench.action.openEditorAtIndex1` stay (no digits / a single switch).
fn is_opaque_token(w: &str) -> bool {
    if w.len() < 32 || !w.chars().all(is_token_char) {
        return false;
    }
    let mut switches = 0;
    let mut prev: Option<bool> = None; // Some(true) = digit, Some(false) = letter
    for c in w.chars().filter(char::is_ascii_alphanumeric) {
        let digit = c.is_ascii_digit();
        if prev.is_some_and(|p| p != digit) {
            switches += 1;
        }
        prev = Some(digit);
    }
    switches >= 3
}

fn is_path_like(w: &str) -> bool {
    let b = w.as_bytes();
    w.starts_with('/')
        || w.starts_with('~')
        || w.starts_with('\\')
        || w.starts_with("./")
        || w.starts_with("../")
        || (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && matches!(b[2], b'\\' | b'/'))
}

// --- 4) MCP servers placeholder expansion + injection ---

fn sync_mcp_servers(dot: &Path, home: &Path, claude_dirs: &[PathBuf]) -> Result<bool> {
    // node availability is not required (Rust handles JSON injection directly)
    let mcp_file = dot.join("claude").join("mcp-servers.json");
    if !mcp_file.exists() {
        return Ok(false);
    }
    let raw = std::fs::read_to_string(&mcp_file)?;

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

    let json_paths = unique_files(mcp_target_files(home, claude_dirs));
    if json_paths.is_empty() {
        return Ok(false);
    }

    let mut changed = false;
    for json_path in &json_paths {
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

/// The `.claude.json` files that receive the MCP servers: one per account dir
/// (`~/.claude-<n>`, including one that is a link to `~/.claude`). `~/.claude` itself is
/// not an account config dir: Claude Code only writes `~/.claude/.claude.json` when run
/// with CLAUDE_CONFIG_DIR pointed at it by mistake, so syncing that stray file would only
/// keep it alive.
///
/// The default account (no CLAUDE_CONFIG_DIR) keeps its config in `~/.claude.json`, and it
/// is left out on purpose: the sync replaces `mcpServers` as a whole, which would drop the
/// MCP servers added there by hand.
fn mcp_target_files(home: &Path, claude_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let primary = home.join(".claude");
    claude_dirs
        .iter()
        .filter(|d| **d != primary)
        .map(|d| d.join(".claude.json"))
        .filter(|p| p.exists())
        .collect()
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

/// Identity of a file for de-duplication: its canonical path (symlinks resolved), or the
/// path as given when that fails (missing file, dangling link). Only used as a key.
fn file_key(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// `paths` in order, without later paths that resolve to a file already listed (e.g.
/// `~/.claude-1/settings.json` symlinked to `~/.claude/settings.json`).
fn unique_files(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    paths
        .into_iter()
        .filter(|p| seen.insert(file_key(p)))
        .collect()
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

#[cfg(test)]
mod tests {
    use super::{
        EXPORT_INTERVAL, build_wingetfile, is_account_dir_name, run_throttled, scoop_app_names,
        winget_excluded,
    };
    use serde_json::json;
    use std::cell::Cell;
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime};

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(name: &str) -> Self {
            let p = std::env::temp_dir().join(format!(
                "dotcli-sync-test-{name}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn stamp_mtime(dir: &Path) -> Option<SystemTime> {
        std::fs::metadata(dir.join("sync-scoop.stamp"))
            .and_then(|m| m.modified())
            .ok()
    }

    /// Runs the throttle with an export that reports `outcome`; returns whether the
    /// export closure was called.
    fn attempt(dir: &Path, force: bool, now: SystemTime, outcome: Option<bool>) -> bool {
        let called = Cell::new(false);
        let _ = run_throttled(Some(dir), "scoop", force, now, || {
            called.set(true);
            Ok(outcome)
        });
        called.get()
    }

    fn base() -> SystemTime {
        // Whole seconds: keeps mtime comparisons exact on every filesystem.
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000)
    }

    /// Package IDs of a real `winget export` from a machine whose scoopfile holds the
    /// scoop names in `SCOOP_APPS`.
    const EXPORTED_IDS: &[&str] = &[
        "Amazon.Kindle", "Anki.Anki", "Anthropic.Claude", "AntibodySoftware.WizTree",
        "Anysphere.Cursor", "AutoHotkey.AutoHotkey", "Beeper.Beeper", "Canonical.Ubuntu",
        "Discord.Discord", "Docker.DockerDesktop", "EpicGames.EpicGamesLauncher",
        "EpicGames.EpicOnlineServices", "GitHub.GitHubDesktop", "Google.AntigravityIDE",
        "Google.JapaneseIME", "Google.PlayGames", "Henry++.simplewall",
        "Kenkun.AviUtlExEdit2", "Microsoft.AppInstaller", "Microsoft.DotNet.Native.Runtime",
        "Microsoft.DotNet.SDK.8", "Microsoft.Edge", "Microsoft.OneDrive", "Microsoft.Outlook",
        "Microsoft.Teams", "Microsoft.UI.Xaml.2.7", "Microsoft.UI.Xaml.2.8",
        "Microsoft.VCLibs.14", "Microsoft.VCLibs.Desktop.14", "Microsoft.VCRedist.2010.x64",
        "Microsoft.VCRedist.2010.x86", "Microsoft.VCRedist.2012.x64",
        "Microsoft.VCRedist.2012.x86", "Microsoft.VCRedist.2013.x64",
        "Microsoft.VCRedist.2013.x86", "Microsoft.VCRedist.2015+.x64",
        "Microsoft.VCRedist.2015+.x86", "Microsoft.VisualStudio.2019.BuildTools",
        "Microsoft.VisualStudio.BuildTools", "Microsoft.WSL", "Microsoft.WindowsADK",
        "Microsoft.WindowsAppRuntime.1.5", "Microsoft.WindowsAppRuntime.1.6",
        "Microsoft.WindowsAppRuntime.1.7", "Microsoft.WindowsAppRuntime.1.8",
        "Microsoft.WindowsAppRuntime.2", "Microsoft.WindowsSDK.10.0.26100",
        "Microsoft.WindowsTerminal", "Microsoft.WingetCreate", "Notion.NotionCalendar",
        "Nvidia.PhysX", "Oracle.VirtualBox", "ParadoxInteractive.ParadoxLauncher",
        "Parsec.Parsec", "Parsec.ParsecVDD", "Perplexity.Comet", "Python.Launcher",
        "SlackTechnologies.Slack", "Spotify.Spotify", "XavierRoche.HTTrack", "ZAP.ZAP",
        "ZhipuAI.AutoClaw", "Zoom.Zoom.EXE", "Zulip.Zulip", "junegunn.fzf",
    ];
    const SCOOP_APPS: &[&str] = &[
        "autohotkey", "discord", "epic-games-launcher", "fzf", "git", "parsec-np", "zoom",
    ];

    fn scoop_set(names: &[&str]) -> HashSet<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    fn export_of(ids: &[&str]) -> serde_json::Value {
        let pkgs: Vec<_> = ids.iter().map(|id| json!({ "PackageIdentifier": id })).collect();
        json!({ "Sources": [
            { "SourceDetails": { "Name": "msstore" },
              "Packages": [{ "PackageIdentifier": "9NBLGGH4NNS1" }] },
            { "SourceDetails": { "Name": "winget" }, "Packages": pkgs },
        ]})
    }

    fn wingetfile_ids(export: &serde_json::Value, scoop: &HashSet<String>) -> Vec<String> {
        let out: serde_json::Value =
            serde_json::from_str(&build_wingetfile(export, scoop).unwrap()).unwrap();
        out["apps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["Id"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn winget_exclude_prefix_rules_match_only_ids_below_them() {
        let none = HashSet::new();
        for id in [
            "Microsoft.VCRedist.2013.x86",
            "Microsoft.VCRedist.2015+.x86",
            "Microsoft.VCLibs.Desktop.14",
            "Microsoft.UI.Xaml.2.8",
            "Microsoft.WindowsAppRuntime.2",
            "microsoft.windowsappruntime.1.8",
        ] {
            assert!(winget_excluded(id, &none), "{id}");
        }
        for id in [
            "Microsoft.VCRedist.2015+.x64",
            "microsoft.vcredist.2015+.X64",
            "Microsoft.WindowsADK",
            "Microsoft.WindowsSDK.10.0.26100",
            "Microsoft.WindowsTerminal",
            "Microsoft.UI.XamlExtra",
        ] {
            assert!(!winget_excluded(id, &none), "{id}");
        }
    }

    #[test]
    fn winget_exclude_exact_rules_leave_longer_ids() {
        let none = HashSet::new();
        for id in [
            "Microsoft.Edge",
            "MICROSOFT.EDGE",
            "Microsoft.OneDrive",
            "Microsoft.AppInstaller",
            "Microsoft.WSL",
            "Canonical.Ubuntu",
            "Microsoft.VisualStudio.2019.BuildTools",
            "Amazon.AWSCLI",
        ] {
            assert!(winget_excluded(id, &none), "{id}");
        }
        for id in [
            "Microsoft.Edge.Beta",
            "Microsoft.EdgeWebView2Runtime.Extra",
            "Microsoft.VisualStudio.BuildTools",
            "Microsoft.DotNet.SDK.8",
            "Canonical.Ubuntu.2404",
        ] {
            assert!(!winget_excluded(id, &none), "{id}");
        }
    }

    #[test]
    fn winget_scoop_duplicates_drop_only_when_scoop_has_the_app() {
        let scoop = scoop_set(SCOOP_APPS);
        assert!(winget_excluded("junegunn.fzf", &scoop));
        assert!(winget_excluded("Parsec.Parsec", &scoop));
        assert!(!winget_excluded("Parsec.ParsecVDD", &scoop), "exact ID match only");
        let without = scoop_set(&["git"]);
        assert!(!winget_excluded("junegunn.fzf", &without));
        assert!(!winget_excluded("Zoom.Zoom.EXE", &without));
    }

    #[test]
    fn wingetfile_from_export_drops_runtimes_os_wsl_and_scoop_duplicates() {
        let ids = wingetfile_ids(&export_of(EXPORTED_IDS), &scoop_set(SCOOP_APPS));
        assert_eq!(EXPORTED_IDS.len(), 65);
        assert_eq!(ids.len(), 34, "{ids:#?}");
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
        for gone in [
            "Microsoft.VCRedist.2015+.x86",
            "Microsoft.WindowsAppRuntime.1.5",
            "Microsoft.UI.Xaml.2.7",
            "Microsoft.VCLibs.14",
            "Microsoft.DotNet.Native.Runtime",
            "EpicGames.EpicOnlineServices",
            "Nvidia.PhysX",
            "Microsoft.AppInstaller",
            "Microsoft.Edge",
            "Microsoft.OneDrive",
            "Microsoft.WSL",
            "Canonical.Ubuntu",
            "Microsoft.VisualStudio.2019.BuildTools",
            "junegunn.fzf",
            "AutoHotkey.AutoHotkey",
            "Discord.Discord",
            "EpicGames.EpicGamesLauncher",
            "Zoom.Zoom.EXE",
            "Parsec.Parsec",
            "9NBLGGH4NNS1",
        ] {
            assert!(!ids.iter().any(|i| i == gone), "{gone} must be dropped");
        }
        for kept in [
            "Microsoft.VCRedist.2015+.x64",
            "Microsoft.VisualStudio.BuildTools",
            "Microsoft.DotNet.SDK.8",
            "Microsoft.WindowsTerminal",
            "Microsoft.WindowsSDK.10.0.26100",
            "Microsoft.WindowsADK",
            "Parsec.ParsecVDD",
            "Python.Launcher",
        ] {
            assert!(ids.iter().any(|i| i == kept), "{kept} must stay");
        }
        // Without a scoopfile the six scoop duplicates stay in.
        let ids = wingetfile_ids(&export_of(EXPORTED_IDS), &HashSet::new());
        assert_eq!(ids.len(), 40);
    }

    #[test]
    fn scoop_app_names_reads_lowercased_names_and_tolerates_missing_file() {
        let t = TempDir::new("scoopnames");
        assert!(scoop_app_names(&t.0).is_empty());
        std::fs::create_dir_all(t.0.join("install")).unwrap();
        std::fs::write(t.0.join("install/scoopfile.json"), "not json").unwrap();
        assert!(scoop_app_names(&t.0).is_empty());
        std::fs::write(
            t.0.join("install/scoopfile.json"),
            r#"{"buckets":[{"Name":"main"}],"apps":[{"Name":"FZF","Source":"main"},{"Name":"zoom"}]}"#,
        )
        .unwrap();
        assert_eq!(scoop_app_names(&t.0), scoop_set(&["fzf", "zoom"]));
    }

    #[test]
    fn export_runs_without_stamp_then_skips_until_interval_passes() {
        let t = TempDir::new("interval");
        let dir = t.0.join("nested");
        assert!(attempt(&dir, false, base(), Some(false)), "first run must export");
        assert_eq!(stamp_mtime(&dir), Some(base()));
        let soon = base() + Duration::from_secs(60);
        assert!(!attempt(&dir, false, soon, Some(true)), "fresh stamp must skip");
        assert_eq!(stamp_mtime(&dir), Some(base()), "a skip must not move the stamp");
        let later = base() + EXPORT_INTERVAL;
        assert!(attempt(&dir, false, later, Some(false)), "expired stamp must export");
        assert_eq!(stamp_mtime(&dir), Some(later));
    }

    #[test]
    fn force_exports_despite_fresh_stamp() {
        let t = TempDir::new("force");
        assert!(attempt(&t.0, false, base(), Some(false)));
        let soon = base() + Duration::from_secs(60);
        assert!(attempt(&t.0, true, soon, Some(false)));
        assert_eq!(stamp_mtime(&t.0), Some(soon));
    }

    #[test]
    fn failed_export_leaves_no_fresh_stamp() {
        let t = TempDir::new("fail-first");
        assert!(attempt(&t.0, false, base(), None));
        assert_eq!(stamp_mtime(&t.0), None, "failed first export must remove its claim");

        let errored = run_throttled(Some(&t.0), "scoop", false, base(), || {
            anyhow::bail!("boom")
        });
        assert!(errored.is_err());
        assert_eq!(stamp_mtime(&t.0), None, "an export error must remove its claim");
    }

    #[test]
    fn failed_export_restores_previous_stamp() {
        let t = TempDir::new("fail-restore");
        assert!(attempt(&t.0, false, base(), Some(false)));
        let later = base() + EXPORT_INTERVAL + Duration::from_secs(1);
        assert!(attempt(&t.0, false, later, None));
        assert_eq!(stamp_mtime(&t.0), Some(base()), "failed retry must restore the old mtime");
        assert!(attempt(&t.0, false, later, Some(false)), "next start must retry");
    }

    #[test]
    fn stamp_in_the_future_counts_as_stale() {
        let t = TempDir::new("future");
        let future = base() + Duration::from_secs(3600);
        assert!(attempt(&t.0, false, future, Some(false)));
        assert!(attempt(&t.0, false, base(), Some(false)));
        assert_eq!(stamp_mtime(&t.0), Some(base()));
    }

    #[test]
    fn missing_stamp_dir_does_not_block_export() {
        let called = Cell::new(false);
        let r = run_throttled(None, "scoop", false, base(), || {
            called.set(true);
            Ok(Some(true))
        });
        assert!(called.get());
        assert!(r.unwrap());
    }

    #[test]
    fn account_dir_name_accepts_numbered_dirs_only() {
        assert!(is_account_dir_name(".claude-1"));
        assert!(is_account_dir_name(".claude-12"));
        assert!(!is_account_dir_name(".claude"));
        assert!(!is_account_dir_name(".claude-"));
        assert!(!is_account_dir_name(".claude-browsers"));
        assert!(!is_account_dir_name(".claude-1.lock"));
        assert!(!is_account_dir_name(".claude-4.lock"));
        assert!(!is_account_dir_name(".claude-1a"));
        assert!(!is_account_dir_name(".claude-１"));
    }

    // --- settings / keybindings sync on a temp home ---

    const PREV_V1: &str = r#"{"bindings":[{"context":"Chat","bindings":{"a":"x","b":"y"}}]}"#;
    const TEMPLATE_V2: &str = r#"{"bindings":[{"context":"Chat","bindings":{"a":"x2"}}]}"#;
    const LOCAL: &str = r#"{"bindings":[{"context":"Chat","bindings":{"a":"x","b":"y","c":"mine"}}]}"#;

    /// `<tmp>/dot/claude/{keybindings,settings}.json` and `<tmp>/home` with `.claude`
    /// and `.claude-1`, each holding a real keybindings.json; prev only in `.claude`.
    fn temp_home(name: &str) -> (TempDir, PathBuf, PathBuf) {
        let tmp = TempDir::new(name);
        let dot = tmp.0.join("dot");
        let home = tmp.0.join("home");
        std::fs::create_dir_all(dot.join("claude")).unwrap();
        std::fs::write(dot.join("claude/keybindings.json"), TEMPLATE_V2).unwrap();
        std::fs::write(dot.join("claude/settings.json"), r#"{"k":1}"#).unwrap();
        for d in [".claude", ".claude-1", ".claude-browsers", ".claude-1.lock"] {
            std::fs::create_dir_all(home.join(d)).unwrap();
        }
        for d in [".claude", ".claude-1"] {
            std::fs::write(home.join(d).join("keybindings.json"), LOCAL).unwrap();
        }
        std::fs::write(home.join(".claude/.dotcli-keybindings-prev.json"), PREV_V1).unwrap();
        (tmp, dot, home)
    }

    fn chat_keys(path: &Path) -> Vec<String> {
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let mut keys: Vec<String> = v["bindings"][0]["bindings"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        keys.sort();
        keys
    }

    fn assert_template_removal_reaches_every_dir(home: &Path, dot: &Path) {
        let dirs = super::claude_dirs(home);
        super::sync_claude_keybindings(dot, &dirs).unwrap();
        for d in [".claude", ".claude-1"] {
            assert_eq!(
                chat_keys(&home.join(d).join("keybindings.json")),
                ["a", "c"],
                "{d}: b was removed from the template"
            );
        }
        let prev = std::fs::read_to_string(home.join(".claude/.dotcli-keybindings-prev.json"));
        assert_eq!(prev.unwrap().trim_end(), TEMPLATE_V2);
    }

    #[test]
    fn claude_dirs_lists_primary_and_numbered_accounts_only() {
        let (_tmp, _dot, home) = temp_home("dirs");
        std::fs::create_dir_all(home.join(".claude-10")).unwrap();
        let names: Vec<String> = super::claude_dirs(&home)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, [".claude", ".claude-1", ".claude-10"]);
    }

    #[test]
    fn keybinding_removal_reaches_dirs_sharing_prev_by_hard_link() {
        let (_tmp, dot, home) = temp_home("kb-hard");
        std::fs::hard_link(
            home.join(".claude/.dotcli-keybindings-prev.json"),
            home.join(".claude-1/.dotcli-keybindings-prev.json"),
        )
        .unwrap();
        assert_template_removal_reaches_every_dir(&home, &dot);
    }

    #[cfg(windows)]
    #[test]
    fn keybinding_removal_reaches_dirs_sharing_prev_by_symlink() {
        let (_tmp, dot, home) = temp_home("kb-sym");
        if let Err(e) = std::os::windows::fs::symlink_file(
            home.join(".claude/.dotcli-keybindings-prev.json"),
            home.join(".claude-1/.dotcli-keybindings-prev.json"),
        ) {
            // The hard-link test covers the same sharing; symlinks need Developer Mode.
            eprintln!("symlink unavailable ({e}); covered by the hard-link test");
            return;
        }
        assert_template_removal_reaches_every_dir(&home, &dot);
        assert!(
            std::fs::symlink_metadata(home.join(".claude-1/.dotcli-keybindings-prev.json"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn keybinding_removal_with_separate_prev_files() {
        let (_tmp, dot, home) = temp_home("kb-sep");
        std::fs::write(home.join(".claude-1/.dotcli-keybindings-prev.json"), PREV_V1).unwrap();
        assert_template_removal_reaches_every_dir(&home, &dot);
        let prev1 = std::fs::read_to_string(home.join(".claude-1/.dotcli-keybindings-prev.json"));
        assert_eq!(prev1.unwrap().trim_end(), TEMPLATE_V2);
    }

    #[test]
    fn second_sync_rewrites_nothing() {
        let (_tmp, dot, home) = temp_home("idem");
        std::fs::hard_link(
            home.join(".claude/.dotcli-keybindings-prev.json"),
            home.join(".claude-1/.dotcli-keybindings-prev.json"),
        )
        .unwrap();
        let dirs = super::claude_dirs(&home);
        super::sync_claude_settings(&dot, &dirs).unwrap();
        super::sync_claude_keybindings(&dot, &dirs).unwrap();

        let files: Vec<PathBuf> = [".claude", ".claude-1"]
            .iter()
            .flat_map(|d| {
                ["settings.json", "keybindings.json", ".dotcli-keybindings-prev.json"]
                    .map(|f| home.join(d).join(f))
            })
            .collect();
        for f in &files {
            std::fs::File::options()
                .write(true)
                .open(f)
                .unwrap()
                .set_modified(base())
                .unwrap();
        }
        super::sync_claude_settings(&dot, &dirs).unwrap();
        super::sync_claude_keybindings(&dot, &dirs).unwrap();
        for f in &files {
            let m = std::fs::metadata(f).unwrap().modified().unwrap();
            assert_eq!(m, base(), "{} was rewritten", f.display());
        }
        // Non-account dirs got nothing.
        for d in [".claude-browsers", ".claude-1.lock"] {
            assert_eq!(std::fs::read_dir(home.join(d)).unwrap().count(), 0, "{d}");
        }
    }

    #[test]
    fn unique_files_drops_paths_to_the_same_file() {
        let tmp = TempDir::new("unique");
        let a = tmp.0.join("a.json");
        std::fs::write(&a, "{}").unwrap();
        let missing = tmp.0.join("missing.json");
        let mut paths = vec![a.clone(), missing.clone(), tmp.0.join(".").join("a.json")];
        #[cfg(windows)]
        {
            // A junction needs no privilege (file symlinks need Developer Mode).
            let junction = tmp.0.join("junction");
            let ok = std::process::Command::new("cmd")
                .args(["/d", "/c", "mklink", "/J"])
                .arg(&junction)
                .arg(&tmp.0)
                .stdout(std::process::Stdio::null())
                .status()
                .unwrap()
                .success();
            assert!(ok, "mklink /J failed");
            paths.push(junction.join("a.json"));
            paths.push(junction.join("missing.json"));
            let link = tmp.0.join("link.json");
            if std::os::windows::fs::symlink_file(&a, &link).is_ok() {
                paths.push(link);
            }
        }
        let n = paths.len();
        let unique = super::unique_files(paths);
        assert!(n >= 5);
        // A missing file cannot be resolved, so the junction path to it stays distinct.
        #[cfg(windows)]
        assert_eq!(unique, [a, missing, tmp.0.join("junction").join("missing.json")]);
        #[cfg(not(windows))]
        assert_eq!(unique, [a, missing]);
    }

    #[cfg(windows)]
    #[test]
    fn account_dir_junction_to_primary_is_synced_once() {
        let (_tmp, dot, home) = temp_home("junction");
        let ok = std::process::Command::new("cmd")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(home.join(".claude-2"))
            .arg(home.join(".claude"))
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap()
            .success();
        assert!(ok, "mklink /J failed");
        let dirs = super::claude_dirs(&home);
        assert_eq!(dirs.len(), 3);
        super::sync_claude_settings(&dot, &dirs).unwrap();
        super::sync_claude_keybindings(&dot, &dirs).unwrap();
        // .claude-2 is .claude: its prev was read before the update, so b is removed.
        assert_eq!(chat_keys(&home.join(".claude-2/keybindings.json")), ["a", "c"]);
        // .claude-1 has no prev of its own: a first sync only merges.
        assert_eq!(chat_keys(&home.join(".claude-1/keybindings.json")), ["a", "b", "c"]);
        let settings = std::fs::read_to_string(home.join(".claude/settings.json")).unwrap();
        assert!(settings.contains("\"k\": 1"));
    }

    fn mcp_server_names(path: &Path) -> Vec<String> {
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        v["mcpServers"].as_object().unwrap().keys().cloned().collect()
    }

    #[test]
    fn mcp_servers_reach_account_dirs_but_not_primary_dir() {
        let (tmp, dot, home) = temp_home("mcp");
        // An absolute command path that exists, so the server is not filtered out.
        let exe = tmp.0.join("server.exe");
        std::fs::write(&exe, "").unwrap();
        let exe = exe.to_string_lossy().replace('\\', "/");
        std::fs::write(
            dot.join("claude/mcp-servers.json"),
            format!(r#"{{"tool":{{"command":"{exe}"}}}}"#),
        )
        .unwrap();
        let stale = r#"{"mcpServers":{"old":{"command":"x"}},"keep":1}"#;
        for f in [".claude/.claude.json", ".claude-1/.claude.json", ".claude.json"] {
            std::fs::write(home.join(f), stale).unwrap();
        }
        let dirs = super::claude_dirs(&home);
        assert!(super::sync_mcp_servers(&dot, &home, &dirs).unwrap());

        let account = home.join(".claude-1/.claude.json");
        assert_eq!(mcp_server_names(&account), ["tool"]);
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&account).unwrap()).unwrap();
        assert_eq!(v["keep"], 1, "keys other than mcpServers are kept");
        // ~/.claude is not an account dir; ~/.claude.json (the default account) is left
        // out so its hand-added servers stay.
        for f in [".claude/.claude.json", ".claude.json"] {
            assert_eq!(std::fs::read_to_string(home.join(f)).unwrap(), stale, "{f}");
        }
        // Nothing left to change on a second run.
        assert!(!super::sync_mcp_servers(&dot, &home, &dirs).unwrap());
    }

    mod vscode {
        use super::super::{
            REDACTED, export_vscode, list_vscode_extensions, normalize_extensions, parse_jsonc,
            redact_secrets, string_has_secret, vscode_user_dir,
        };
        use super::TempDir;
        use serde_json::{Value, json};
        use std::path::Path;

        // Token-shaped test values are assembled at run time so the source holds no
        // literal that secret scanners would flag. All of them are fake.
        fn fake_ghp() -> String {
            format!("ghp_{}", "aB3".repeat(12))
        }

        fn fake_secrets() -> Vec<String> {
            let ghp = fake_ghp();
            vec![
                ghp.clone(),
                format!("github_pat_{}", "x9Y".repeat(10)),
                format!("gho_{}", "Qw8".repeat(12)),
                format!("sk-{}", "proj7Q".repeat(6)),
                format!("xoxb-{}", "1234-5678".repeat(2)),
                format!("AKIA{}", "Q7".repeat(8)),
                format!("prefixAKIA{}", "Z1".repeat(8)),
                format!("AIza{}", "Sy0-x".repeat(7)),
                format!("eyJ{}.eyJ{}.{}", "hbGciOiJIUzI1NiJ9", "zdWIiOiIxIn0", "c2lnbmF0dXJl"),
                // Opaque strings: hex, base64-like, with separators.
                format!("{}9f", "0123456789abcdef".repeat(2)),
                "Zm9vYmFy4K".repeat(4),
                format!("{}==", "a1B2c3D4/e5F6g7H8+i9J0".repeat(2)),
                // Embedded in a longer string.
                format!("Bearer {ghp}"),
                format!("--api={}", "0123456789abcdef".repeat(3)),
                format!("'{ghp}'"),
                // URLs with credentials.
                "https://example.com/cb?token=abc".into(),
                "https://example.com/x?a=1&key=abc".into(),
                "https://example.com/x#access_token=abc".into(),
                "https://user:pw@example.com/".into(),
                format!("https://{ghp}@github.com/o/r.git"),
                format!("https://example.com/{ghp}/x"),
                format!("https://example.com/?q={ghp}"),
            ]
        }

        const NOT_SECRETS: &[&str] = &[
            "",
            "sk-SK",
            "sk-learn-something-without-digits",
            "ghp_",
            "eyJ",
            "AKIA",
            "ms-vscode-remote.remote-ssh-edit",
            "ms-toolsai.vscode-jupyter-cell-tags",
            "workbench.action.openEditorAtIndex1",
            "editor.action.insertCursorAbove",
            "editorTextFocus && !editorReadonly && config.foo.bar2",
            "HackGen Console NF, Consolas, monospace",
            "#1e1e1e",
            "${workspaceFolder}/build/a1b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6",
            r"C:\Users\me\AppData\Local\Programs\abc123def456ghi789jkl012mno345",
            "/c/Users/me/a1b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6q7",
            "~/a1b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6",
            "https://github.com/a1b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6/repo?tab=readme",
            "https://example.com/search?q=hello&page=2",
            "-interaction=nonstopmode",
            "%DOC%",
            "lualatex -synctex=1 -interaction=nonstopmode -file-line-error %DOC%",
        ];

        #[test]
        fn string_rule_redacts_tokens_and_keeps_ordinary_values() {
            for s in fake_secrets() {
                assert!(string_has_secret(&s), "should redact: {s}");
            }
            for s in NOT_SECRETS {
                assert!(!string_has_secret(s), "should keep: {s}");
            }
        }

        #[test]
        fn key_rule_redacts_whole_values_but_keeps_flags_and_numbers() {
            let ghp = fake_ghp();
            let original = json!({
                "codic.ACCESS_TOKEN": "plainvalue",
                "x.auth.enabled": true,
                "x.sessionTimeout": 30,
                "x.password": "",
                "x.privateKey": null,
                "x.Credentials": { "user": "u", "nested": 1 },
                "[python]": { "editor.tabSize": 4, "my.apiKey": "abc" },
                "tools": [{ "name": "x", "args": ["--key", ghp] }],
                "**/a~b": { "Cookie": ["x"] },
                "plain": "keep",
            });
            let mut v = original.clone();
            let paths = redact_secrets(&mut v);
            assert_eq!(
                paths,
                [
                    "/codic.ACCESS_TOKEN",
                    "/x.Credentials",
                    "/[python]/my.apiKey",
                    "/tools/0/args/1",
                    "/**~1a~0b/Cookie",
                ]
            );
            for p in &paths {
                assert!(original.pointer(p).is_some(), "{p} is a JSON pointer");
                assert_eq!(v.pointer(p).unwrap(), REDACTED, "{p}");
            }
            assert_eq!(v["x.auth.enabled"], true);
            assert_eq!(v["x.sessionTimeout"], 30);
            assert_eq!(v["x.password"], "");
            assert_eq!(v["x.privateKey"], Value::Null);
            assert_eq!(v["[python]"]["editor.tabSize"], 4);
            assert_eq!(v["tools"][0]["args"][0], "--key");
            assert_eq!(v["plain"], "keep");
        }

        #[test]
        fn jsonc_comments_trailing_commas_and_strings() {
            let cases: &[(&str, Value)] = &[
                (
                    "// head\n{\"a\": 1, // c\n \"b\": [1, 2,],\n}",
                    json!({ "a": 1, "b": [1, 2] }),
                ),
                (
                    r#"{"url": "https://x//y", "c": "/* not */", "d": "a,}"}"#,
                    json!({ "url": "https://x//y", "c": "/* not */", "d": "a,}" }),
                ),
                ("{\"p\": \"C:\\\\\" // c\n}", json!({ "p": "C:\\" })),
                (
                    r#"{"q": "say \"//hi\", ]"}"#,
                    json!({ "q": "say \"//hi\", ]" }),
                ),
                (
                    "\u{feff}{\"a\": [1, /* x */ 2, /* y */ ]}",
                    json!({ "a": [1, 2] }),
                ),
                ("{\"a\": 1,\n  // last\n}", json!({ "a": 1 })),
                ("{\"a\": 1,\r\n  /* multi\r\n line */\r\n}", json!({ "a": 1 })),
                ("{\"a\": 1} // eof", json!({ "a": 1 })),
                ("{\"a\": \"日本語 // コメントではない\"}", json!({ "a": "日本語 // コメントではない" })),
                ("// only a comment\n[]", json!([])),
                ("{\"a\": 1, \"a\": 2}", json!({ "a": 2 })),
            ];
            for (src, want) in cases {
                assert_eq!(&parse_jsonc(src).unwrap(), want, "{src}");
            }
            assert!(parse_jsonc("{\"a\": }").is_err());
        }

        #[test]
        fn extensions_are_lowercased_sorted_and_deduplicated() {
            let out = "Foo.Bar\r\nfoo.bar\n\nms-python.python\n  A.b  \nsome message here\nnodot\n";
            assert_eq!(normalize_extensions(out), "a.b\nfoo.bar\nms-python.python");
            assert_eq!(normalize_extensions(""), "");
        }

        fn read(p: &Path) -> String {
            std::fs::read_to_string(p).unwrap()
        }

        #[test]
        fn export_writes_redacted_files_once_and_keeps_them_without_input() {
            let t = TempDir::new("vscode-export");
            let user = t.0.join("User");
            let out = t.0.join("out");
            std::fs::create_dir_all(&user).unwrap();
            let ghp = fake_ghp();
            std::fs::write(
                user.join("settings.json"),
                format!(
                    "{{\r\n  // comment\r\n  \"codic.ACCESS_TOKEN\": \"{ghp}\",\r\n  \"editor.tabSize\": 2,\r\n}}\r\n"
                ),
            )
            .unwrap();
            std::fs::write(
                user.join("keybindings.json"),
                "// header\n[{ \"key\": \"ctrl+k\", \"command\": \"x.y\", },]\n",
            )
            .unwrap();

            let r = export_vscode(&out, Some("B.c\na.b\n"), Some(&user)).unwrap();
            assert!(r.produced && r.changed);
            assert_eq!(r.redacted, ["settings.json#/codic.ACCESS_TOKEN"]);
            assert_eq!(read(&out.join("extensions.txt")), "a.b\nb.c\n");
            let settings = read(&out.join("settings.json"));
            assert!(!settings.contains(&ghp));
            let v: Value = serde_json::from_str(&settings).unwrap();
            assert_eq!(v, json!({ "codic.ACCESS_TOKEN": REDACTED, "editor.tabSize": 2 }));
            let kb: Value = serde_json::from_str(&read(&out.join("keybindings.json"))).unwrap();
            assert_eq!(kb, json!([{ "key": "ctrl+k", "command": "x.y" }]));

            // Same input: nothing rewritten.
            let r = export_vscode(&out, Some("a.b\nB.c"), Some(&user)).unwrap();
            assert!(r.produced && !r.changed);

            // No code / empty list / unparsable settings: existing files stay.
            std::fs::write(user.join("settings.json"), "{ broken").unwrap();
            std::fs::remove_file(user.join("keybindings.json")).unwrap();
            let r = export_vscode(&out, Some("\n"), Some(&user)).unwrap();
            assert!(!r.produced && !r.changed);
            let r = export_vscode(&out, None, None).unwrap();
            assert!(!r.produced && !r.changed);
            assert_eq!(read(&out.join("extensions.txt")), "a.b\nb.c\n");
            assert_eq!(read(&out.join("settings.json")), settings);
            assert!(out.join("keybindings.json").exists());
        }

        fn string_leaves<'a>(v: &'a Value, acc: &mut Vec<&'a str>) {
            match v {
                Value::String(s) => acc.push(s),
                Value::Array(a) => a.iter().for_each(|x| string_leaves(x, acc)),
                Value::Object(o) => o.values().for_each(|x| string_leaves(x, acc)),
                _ => {}
            }
        }

        /// Generates the snapshot from the live VS Code profile into
        /// $DOTCLI_VSCODE_EXPORT_OUT and checks that no redacted value reaches any output
        /// file. Prints only counts, key paths and booleans, never values.
        #[test]
        #[ignore = "reads the live VS Code profile; set DOTCLI_VSCODE_EXPORT_OUT"]
        fn live_export_leaks_no_redacted_value() {
            let Some(out) = std::env::var_os("DOTCLI_VSCODE_EXPORT_OUT") else {
                panic!("DOTCLI_VSCODE_EXPORT_OUT is not set");
            };
            let out = Path::new(&out).to_path_buf();
            let user = vscode_user_dir().unwrap();
            let ext = list_vscode_extensions();
            println!("code --list-extensions succeeded: {}", ext.is_some());
            let r = export_vscode(&out, ext.as_deref(), Some(&user)).unwrap();
            println!("produced={} changed={} redacted={}", r.produced, r.changed, r.redacted.len());
            for p in &r.redacted {
                println!("redacted: {p}");
            }
            let outputs: Vec<String> = ["extensions.txt", "settings.json", "keybindings.json"]
                .iter()
                .map(|f| std::fs::read_to_string(out.join(f)).unwrap_or_default())
                .collect();
            for name in ["settings.json", "keybindings.json"] {
                let Ok(src) = std::fs::read_to_string(user.join(name)) else {
                    println!("{name}: not found");
                    continue;
                };
                let orig = parse_jsonc(&src).unwrap();
                let top = match &orig {
                    Value::Object(m) => m.len(),
                    Value::Array(a) => a.len(),
                    _ => 0,
                };
                println!("{name}: top-level entries {top}");
                let prefix = format!("{name}#");
                for p in r.redacted.iter().filter_map(|p| p.strip_prefix(&prefix)) {
                    let mut leaves = Vec::new();
                    string_leaves(orig.pointer(p).unwrap(), &mut leaves);
                    // Short leaves (e.g. "a") occur anywhere; they cannot be secrets.
                    let absent = leaves
                        .iter()
                        .filter(|s| s.len() >= 8)
                        .all(|s| outputs.iter().all(|o| !o.contains(*s)));
                    println!("{name}#{p}: original value absent from all outputs: {absent}");
                    assert!(absent, "a redacted value leaked");
                }
            }
            let codic = std::fs::read_to_string(user.join("settings.json"))
                .ok()
                .and_then(|s| parse_jsonc(&s).ok())
                .and_then(|v| v.get("codic.ACCESS_TOKEN").and_then(Value::as_str).map(String::from));
            match codic {
                Some(tok) if !tok.is_empty() => {
                    let absent = outputs.iter().all(|o| !o.contains(&tok));
                    println!("codic.ACCESS_TOKEN set in live: true, absent from all outputs: {absent}");
                    assert!(absent, "codic token leaked");
                }
                _ => println!("codic.ACCESS_TOKEN set in live: false"),
            }
        }
    }
}
