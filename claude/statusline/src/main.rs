// Claude Code statusline: 2-line, 3-column with usage info
use serde::Deserialize;
use serde::Serialize;
use std::io::{self, Read};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, fmt::Write};

const BAR_WIDTH: usize = 6;
const FIVE_HOURS: f64 = 18000.0;
const SEVEN_DAYS: f64 = 604800.0;
const LOADING: &str = "             \u{29d7}"; // 6 + 4 spaces + ⧗

// ANSI colors
const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[38;2;255;220;50m";
const CYAN: &str = "\x1b[36m";
const MAGENTA: &str = "\x1b[35m";
const RST: &str = "\x1b[0m";

const ACC_ICONS: &[char] = &['①', '②', '③', '④', '⑤', '⑥', '⑦', '⑧', '⑨'];

#[derive(Deserialize, Default)]
struct Input {
    model: Option<Model>,
    context_window: Option<ContextWindow>,
    rate_limits: Option<RateLimits>,
    transcript_path: Option<String>,
    cost: Option<Cost>,
    effort: Option<Effort>,
}

#[derive(Deserialize, Default)]
struct Effort {
    level: Option<String>,
}

#[derive(Deserialize, Default)]
struct Model {
    display_name: Option<String>,
}

#[derive(Deserialize, Default)]
struct ContextWindow {
    used_percentage: Option<f64>,
    context_window_size: Option<u64>,
    total_input_tokens: Option<u64>,
    current_usage: Option<CurrentUsage>,
}

#[derive(Deserialize, Default)]
struct CurrentUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
}

#[derive(Deserialize, Default)]
struct Cost {
    #[serde(default)]
    total_cost_usd: f64,
    #[serde(default)]
    total_lines_added: u64,
    #[serde(default)]
    total_lines_removed: u64,
}

#[derive(Deserialize, Serialize, Clone, Default, PartialEq, Debug)]
struct RateLimits {
    five_hour: Option<Window>,
    seven_day: Option<Window>,
}

#[derive(Deserialize, Serialize, Clone, Default, PartialEq, Debug)]
struct Window {
    used_percentage: Option<f64>,
    resets_at: Option<serde_json::Value>,
}

fn bar(pct: i32) -> String {
    let p = pct.clamp(0, 100);
    let mut filled = (p * BAR_WIDTH as i32 / 100) as usize;
    if p > 0 && filled == 0 {
        filled = 1;
    }
    let empty = BAR_WIDTH - filled;
    "\u{2593}".repeat(filled) + &"\u{2591}".repeat(empty)
}

fn stat(label: &str, pct: i32) -> String {
    let p = pct.clamp(0, 100);
    format!("{}{}{:3}%", label, bar(p), p)
}

fn fmt_num(n: u64) -> String {
    if n >= 10000 {
        format!("{}k", n / 1000)
    } else if n >= 1000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}

fn now_unix() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}

fn parse_resets_at(v: &serde_json::Value) -> Option<f64> {
    match v {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => parse_iso8601(s),
        _ => None,
    }
}

fn parse_iso8601(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.len() < 19 {
        return None;
    }
    let year: i64 = s[0..4].parse().ok()?;
    let month: i64 = s[5..7].parse().ok()?;
    let day: i64 = s[8..10].parse().ok()?;
    let hour: i64 = s[11..13].parse().ok()?;
    let min: i64 = s[14..16].parse().ok()?;
    let sec: i64 = s[17..19].parse().ok()?;
    let mut y = year;
    let mut m = month;
    if m <= 2 {
        y -= 1;
        m += 12;
    }
    let days = 365 * y + y / 4 - y / 100 + y / 400 + (153 * (m - 3) + 2) / 5 + day - 719469;
    Some((days * 86400 + hour * 3600 + min * 60 + sec) as f64)
}

fn elapsed_pct(resets_at: &Option<serde_json::Value>, window_secs: f64) -> i32 {
    resets_at
        .as_ref()
        .and_then(|v| parse_resets_at(v))
        .map(|reset| {
            let remaining = (reset - now_unix()).max(0.0);
            ((window_secs - remaining) * 100.0 / window_secs).floor() as i32
        })
        .unwrap_or(0)
}

// ========================================
// Auto-compact threshold
// ========================================

// Mirrors Claude Code's auto-compact threshold (extracted from v2.1.288):
//   window    = context_window_size, capped by CLAUDE_CODE_AUTO_COMPACT_WINDOW
//   effective = window - min(max_output_tokens, 20000)
//   threshold = effective - 13000
//               CLAUDE_AUTOCOMPACT_PCT_OVERRIDE can only lower it
//               (min of effective * pct / 100 and the default)
//   auto-compact disabled -> effective (where the blocking limit applies)
// max_output_tokens is not passed to statuslines; every current model
// exceeds 20000, so the full reserve is assumed.
// TODO: settings-based sources are not visible here: `autoCompactWindow`
// in settings.json, `autoCompactEnabled` toggled via /config, and
// server-side window experiments. Those fall back to the formula above.
const COMPACT_OUTPUT_RESERVE: u64 = 20_000;
const COMPACT_BUFFER: u64 = 13_000;
const COMPACT_WINDOW_MIN: u64 = 100_000;
const COMPACT_WINDOW_MAX: u64 = 1_000_000;
// Fallback when context_window_size is absent (200k window ratio)
const COMPACT_THRESHOLD_RATIO: f64 = 0.835;

fn env_truthy(name: &str) -> bool {
    env::var(name)
        .map(|v| matches!(v.trim().to_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

fn env_positive_u64(name: &str) -> Option<u64> {
    env::var(name)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|&n| n > 0)
}

fn compact_threshold(context_window_size: u64) -> u64 {
    let window = match env_positive_u64("CLAUDE_CODE_AUTO_COMPACT_WINDOW") {
        Some(n) => context_window_size.min(n.clamp(COMPACT_WINDOW_MIN, COMPACT_WINDOW_MAX)),
        None => context_window_size,
    };
    let effective = window.saturating_sub(COMPACT_OUTPUT_RESERVE);
    if env_truthy("DISABLE_COMPACT") || env_truthy("DISABLE_AUTO_COMPACT") {
        return effective;
    }
    let default = effective.saturating_sub(COMPACT_BUFFER);
    let pct = env::var("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE")
        .ok()
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|p| *p > 0.0 && *p <= 100.0);
    match pct {
        Some(p) => ((effective as f64 * p / 100.0).floor() as u64).min(default),
        None => default,
    }
}

/// Context usage relative to the auto-compact threshold (100 = compaction).
fn context_pct(cw: &ContextWindow) -> Option<i32> {
    let tokens = cw.total_input_tokens.filter(|_| cw.current_usage.is_some());
    let pct = match cw.context_window_size.filter(|&n| n > 0) {
        Some(size) => {
            // used_percentage is integer-rounded; prefer exact tokens
            let tokens = match tokens {
                Some(t) => t as f64,
                None => cw.used_percentage? * size as f64 / 100.0,
            };
            tokens * 100.0 / compact_threshold(size).max(1) as f64
        }
        None => cw.used_percentage? / COMPACT_THRESHOLD_RATIO,
    };
    Some(pct.min(100.0) as i32)
}

// Transcripts can reach hundreds of MB, so only the tail is read: start with
// this window and double it up to the cap while no model is found.
const TRANSCRIPT_TAIL_INITIAL: u64 = 256 * 1024;
const TRANSCRIPT_TAIL_MAX: u64 = 4 * 1024 * 1024;

fn read_model_from_transcript(path: &str) -> Option<String> {
    read_model_from_transcript_tail(path, TRANSCRIPT_TAIL_INITIAL, TRANSCRIPT_TAIL_MAX)
}

/// Model of the last assistant line within the last `max_window` bytes.
fn read_model_from_transcript_tail(path: &str, initial_window: u64, max_window: u64) -> Option<String> {
    use std::io::{Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let mut window = initial_window.max(1).min(len);
    loop {
        // Read one byte before the window so a line starting exactly at the
        // window edge is kept; the partial first line is dropped below.
        let start = len - window;
        let read_from = start.saturating_sub(1);
        file.seek(SeekFrom::Start(read_from)).ok()?;
        let mut buf = Vec::with_capacity((len - read_from) as usize);
        file.by_ref().take(len - read_from).read_to_end(&mut buf).ok()?;
        let bytes = if read_from > 0 {
            match buf.iter().position(|&b| b == b'\n') {
                Some(i) => &buf[i + 1..],
                None => &[][..],
            }
        } else {
            &buf[..]
        };
        if let Some(model) = find_last_assistant_model(&String::from_utf8_lossy(bytes)) {
            return Some(model);
        }
        if start == 0 || window >= max_window {
            return None;
        }
        window = window.saturating_mul(2).min(max_window).min(len);
    }
}

/// Scans lines from the end; unparsable lines (e.g. one still being written)
/// and assistant lines without a model are skipped.
fn find_last_assistant_model(content: &str) -> Option<String> {
    content.lines().rev().find_map(|line| {
        if !line.contains("\"type\"") || !line.contains("\"assistant\"") {
            return None;
        }
        let obj: serde_json::Value = serde_json::from_str(line).ok()?;
        if obj.get("type")?.as_str()? != "assistant" {
            return None;
        }
        obj.get("message")?.get("model")?.as_str().map(|s| s.to_string())
    })
}

// ========================================
// Statusline TOML Rules
// ========================================

/// A `[[model]]` or `[[provider]]` entry: names matched case-insensitively.
#[derive(Deserialize, Default, Clone)]
struct Rule {
    #[serde(default)]
    name: Vec<String>,
    #[serde(default)]
    display: String,
    #[serde(default)]
    color: String,
}

#[derive(Deserialize, Default)]
struct RulesFile {
    #[serde(default)]
    model: Vec<Rule>,
    #[serde(default)]
    provider: Vec<Rule>,
}

// ========================================
// Config dir
// ========================================

/// Claude config dir: CLAUDE_CONFIG_DIR, else USERPROFILE/.claude, else
/// HOME/.claude. An empty variable counts as unset.
fn config_dir() -> Option<String> {
    config_dir_from(|name| env::var(name).ok())
}

fn config_dir_from(get: impl Fn(&str) -> Option<String>) -> Option<String> {
    let get = |name: &str| get(name).filter(|v| !v.is_empty());
    get("CLAUDE_CONFIG_DIR").or_else(|| {
        get("USERPROFILE")
            .or_else(|| get("HOME"))
            .map(|h| format!("{h}/.claude"))
    })
}

// ========================================
// TOML Rule Loading
// ========================================

fn match_rule<'a>(name: &str, rules: &'a [Rule]) -> Option<&'a Rule> {
    rules.iter().find(|r| r.name.iter().any(|n| n.eq_ignore_ascii_case(name)))
}

/// Rules are only consulted for a model the Anthropic pattern does not cover
/// and for a provider badge, so most runs skip reading and parsing the file.
fn rules_needed(model_raw: &str, provider: Option<&str>) -> bool {
    provider.is_some() || anthropic_badge(model_raw).is_none()
}

fn load_rules(dir: Option<&str>) -> RulesFile {
    let content = dir
        .and_then(|d| std::fs::read_to_string(format!("{d}/statusline-rules.toml")).ok())
        .unwrap_or_else(|| include_str!("../statusline-models.toml").to_string());
    toml::from_str::<RulesFile>(&content)
        .unwrap_or_else(|e| {
            eprintln!("Failed to parse statusline rules: {e}");
            RulesFile::default()
        })
}

fn fmt_color(rgb: &str) -> String {
    format!("\x1b[38;2;{}m", rgb.replace(',', ";"))
}

/// Badge (1-glyph icon): trailing space to prevent glyph overlap in terminals
fn fmt_badge(display: &str, color: &str) -> String {
    format!("{}{display}{RST} ", fmt_color(color))
}

/// 最初に現れる数字列（`N.N` または `N`）を抽出
fn extract_version(s: &str) -> Option<String> {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            return Some(chars[start..i].iter().collect());
        }
        i += 1;
    }
    None
}

/// Anthropic モデル名 (`Opus 4.8`, `Sonnet 4.6 (1M context)` 等) を
/// バッジ表示と色へパターン変換する。命名規則が安定している前提で
/// toml への都度追記を不要にするためのもの。非該当なら None。
fn anthropic_badge(name: &str) -> Option<(String, String)> {
    let lower = name.to_lowercase();
    let (sym, color) = if lower.contains("opus") {
        ("\u{1F17E}", "207,144,82") // 🅾
    } else if lower.contains("sonnet") {
        ("\u{1F182}", "180,130,100") // 🆂
    } else if lower.contains("haiku") {
        ("\u{1F177}", "120,180,120") // 🅷
    } else if lower.contains("fable") {
        ("\u{1F175}", "180,130,210") // 🅵
    } else {
        return None;
    };
    let ver = extract_version(name)?;
    let plus = if lower.contains("1m") { "\u{207A}" } else { "" }; // ⁺
    Some((format!("{sym} {ver}{plus}"), color.to_string()))
}

fn model_short_with_rules(display_name: &str, rules: &[Rule]) -> String {
    // Anthropic モデルはパターン解析で自動生成（toml 追記不要）
    if let Some((disp, color)) = anthropic_badge(display_name) {
        return format!("{}{disp}{RST}", fmt_color(&color));
    }
    if let Some(rule) = match_rule(display_name, rules) {
        let color = fmt_color(&rule.color);
        format!("{color}{}{RST}", rule.display)
    } else {
        // 未知モデル
        format!("\x1b[38;2;150;150;150m❓ {display_name}\x1b[0m")
    }
}

/// effort.level → 色付きラベル。effort 非対応モデル(フィールド absent)は呼ばれない。
/// ultracode は xhigh として報告される(Claude Code 仕様)
fn effort_badge(level: &str) -> String {
    let (label, rgb) = match level {
        "low" => ("L", "255,220,50"),
        "medium" => ("M", "80,200,120"),
        "high" => ("H", "80,160,255"),
        "xhigh" => ("XH", "190,120,255"),
        "max" => ("MX", "255,80,80"),
        other => return format!(" \x1b[38;2;150;150;150m{other}{RST}"),
    };
    format!(" {}{label}{RST}", fmt_color(rgb))
}

fn detect_provider() -> Option<String> {
    env::var("ANTHROPIC_BASE_URL")
        .ok()
        .filter(|url| url.to_lowercase().contains("z.ai"))
        .map(|_| "z.ai".into())
}

fn provider_badge_with_rules(name: &str, rules: &[Rule]) -> Option<String> {
    match_rule(name, rules).map(|rule| fmt_badge(&rule.display, &rule.color))
}

// ========================================
// GLM Quota API
// ========================================

const CACHE_TTL_SECONDS: u64 = 60;
// After a failed fetch, skip the network for this long. Without it every run
// past the TTL blocks on the HTTPS timeout while z.ai is down or offline.
const FAIL_CACHE_TTL_SECONDS: u64 = 30;
const GLM_API_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

#[derive(Deserialize, Serialize, Default, Clone)]
struct CachedRateLimits {
    /// Time the rate_limits were last fetched successfully (0 = never).
    /// ycusage reads it to label stale data, so failures leave it unchanged.
    cached_at: u64,
    provider: Option<String>,
    /// None (written as null) until a fetch has succeeded, so readers such
    /// as ycusage treat the file as having no data rather than as 0% usage.
    #[serde(default)]
    rate_limits: Option<RateLimits>,
    /// Time of the last failed fetch (0 = the last attempt succeeded).
    /// Absent in caches written before this field existed.
    #[serde(default)]
    failed_at: u64,
}

/// Whether the cache can be used without calling the API.
fn cache_is_fresh(cached: &CachedRateLimits, now: u64) -> bool {
    if cached.provider.as_deref() != Some("z.ai") {
        return false;
    }
    if cached.cached_at > 0 && now.saturating_sub(cached.cached_at) < CACHE_TTL_SECONDS {
        return true;
    }
    cached.failed_at > 0 && now.saturating_sub(cached.failed_at) < FAIL_CACHE_TTL_SECONDS
}

/// Cache to write after a failed fetch: keeps the previous data and its
/// cached_at, and records the failure so the next runs skip the network.
/// Without a previous success (cached_at 0) there is no data to keep.
fn failure_cache(prev: Option<CachedRateLimits>, now: u64) -> CachedRateLimits {
    let prev = prev.unwrap_or_default();
    CachedRateLimits {
        cached_at: prev.cached_at,
        provider: Some("z.ai".into()),
        rate_limits: if prev.cached_at > 0 { prev.rate_limits } else { None },
        failed_at: now,
    }
}

/// Plain overwrite, not temp + rename: ~/.claude-N/.glm-rate-limits.json is a
/// symlink shared by all accounts, and a rename would replace the link.
fn write_glm_cache(cache_path: &str, data: &CachedRateLimits) {
    if let Ok(json_str) = serde_json::to_string(data) {
        let _ = std::fs::write(cache_path, json_str);
    }
}

/// What get_glm_rate_limits found in the cache file before fetching.
enum GlmCacheRead {
    /// No file, or it could not be read.
    Missing,
    /// The file was read but did not parse, and was modified moments ago.
    /// fs::write truncates before writing, so this can be another run's
    /// write caught midway.
    Unparsable,
    Parsed(CachedRateLimits),
}

/// An unparsable cache last modified longer ago than this is broken rather
/// than a write in progress, and is treated as missing so a failed fetch can
/// replace it. Without this, a file left torn by a killed writer would keep
/// every run blocking on the HTTPS timeout while z.ai is down.
const TORN_WRITE_WINDOW: std::time::Duration = std::time::Duration::from_secs(5);

fn read_glm_cache(path: &str) -> GlmCacheRead {
    let Ok(text) = std::fs::read_to_string(path) else {
        return GlmCacheRead::Missing;
    };
    if let Ok(v) = serde_json::from_str::<CachedRateLimits>(&text) {
        return GlmCacheRead::Parsed(v);
    }
    // Unknown or future mtime counts as recent, which keeps the file.
    let recent = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| std::time::SystemTime::now().duration_since(t).ok())
        .is_none_or(|age| age < TORN_WRITE_WINDOW);
    if recent {
        GlmCacheRead::Unparsable
    } else {
        GlmCacheRead::Missing
    }
}

/// Records a failed fetch and returns the rate limits to display.
/// An unparsable file is left alone: it may be a good cache being written by
/// a concurrent run, and replacing it would drop the last fetched data.
fn glm_fetch_failed(cache_path: &str, prev: GlmCacheRead, now: u64, msg: &str) -> RateLimits {
    if env_truthy("STATUSLINE_DEBUG") {
        eprintln!("{msg}");
    }
    let prev = match prev {
        GlmCacheRead::Unparsable => return RateLimits::default(),
        GlmCacheRead::Missing => None,
        GlmCacheRead::Parsed(c) => Some(c),
    };
    let cache = failure_cache(prev, now);
    write_glm_cache(cache_path, &cache);
    cache.rate_limits.unwrap_or_default()
}

fn get_glm_rate_limits(dir: Option<&str>) -> RateLimits {
    let Some(dir) = dir else {
        return RateLimits::default();
    };
    let cache_path = format!("{dir}/.glm-rate-limits.json");

    let now = now_unix() as u64;

    // Read cache without provider filter (will check after parsing)
    let parsed_cache = read_glm_cache(&cache_path);

    // Fresh z.ai cache (successful, or a recent failure) skips the API
    if let GlmCacheRead::Parsed(ref cached_data) = parsed_cache {
        if cache_is_fresh(cached_data, now) {
            return cached_data.rate_limits.clone().unwrap_or_default();
        }
    }

    // Fetch from GLM API
    let api_key = match env::var("ANTHROPIC_AUTH_TOKEN").ok() {
        Some(k) => k,
        None => {
            // Fallback: read from ~/.claude/.glm-api-key (same resolved dir as cache)
            let key_path = format!("{dir}/.glm-api-key");
            match std::fs::read_to_string(&key_path) {
                Ok(k) => k.trim().to_string(),
                Err(_) => return RateLimits::default(),
            }
        }
    };

    let resp = match ureq::get("https://api.z.ai/api/monitor/usage/quota/limit")
        .set("Authorization", &api_key)
        .timeout(GLM_API_TIMEOUT)
        .call() {
        Ok(r) => r,
        Err(e) => {
            // API failed: use stale cache if available
            let msg = format!("GLM API request failed: {e}");
            return glm_fetch_failed(&cache_path, parsed_cache, now, &msg);
        }
    };

    let body: serde_json::Value = match resp.into_string() {
        Ok(s) => match serde_json::from_str(&s) {
            Ok(v) => v,
            Err(e) => {
                let msg = format!("Failed to parse GLM JSON: {e}, body: {s}");
                return glm_fetch_failed(&cache_path, parsed_cache, now, &msg);
            }
        },
        Err(e) => {
            let msg = format!("Failed to read GLM response body: {e}");
            return glm_fetch_failed(&cache_path, parsed_cache, now, &msg);
        }
    };

    // An invalid token comes back as HTTP 200 with {"code":401,"success":false}
    // and no data; treat it as a failure so the previous numbers are kept.
    let Some(limits) = body.get("data")
        .and_then(|d| d.get("limits"))
        .and_then(|l| l.as_array())
    else {
        let msg = format!("GLM response has no data.limits: {body}");
        return glm_fetch_failed(&cache_path, parsed_cache, now, &msg);
    };

    let mut five_hour_pct: f64 = 0.0;
    let mut five_hour_reset: Option<serde_json::Value> = None;
    let mut seven_day_pct: f64 = 0.0;
    let mut seven_day_reset: Option<serde_json::Value> = None;

    for limit in limits {
        let limit_type = limit.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if limit_type != "TOKENS_LIMIT" {
            continue;
        }
        let unit = limit.get("unit").and_then(|u| u.as_i64()).unwrap_or(0);
        let number = limit.get("number").and_then(|n| n.as_i64()).unwrap_or(0);
        let pct = limit.get("percentage").and_then(|p| p.as_f64()).unwrap_or(0.0);
        let reset_ms = limit.get("nextResetTime").and_then(|r| r.as_u64());

        // unit=3, number=5 → 5h window
        if unit == 3 && number == 5 {
            five_hour_pct = pct;
            if let Some(ms) = reset_ms {
                five_hour_reset = Some(serde_json::json!(ms / 1000));
            }
        }
        // unit=6, number=1 → weekly window
        if unit == 6 && number == 1 {
            seven_day_pct = pct;
            if let Some(ms) = reset_ms {
                seven_day_reset = Some(serde_json::json!(ms / 1000));
            }
        }
    }

    let rl = RateLimits {
        five_hour: Some(Window {
            used_percentage: Some(five_hour_pct),
            resets_at: five_hour_reset,
        }),
        seven_day: Some(Window {
            used_percentage: Some(seven_day_pct),
            resets_at: seven_day_reset,
        }),
    };

    // Write to cache
    let cache_data = CachedRateLimits {
        cached_at: now,
        provider: Some("z.ai".into()),
        rate_limits: Some(rl.clone()),
        failed_at: 0,
    };
    write_glm_cache(&cache_path, &cache_data);

    rl
}

/// Account number from the trailing digits of the config dir name
/// (`.claude-3` -> 3). The default `.claude` gives 0.
fn acc_from_dir(dir: Option<&str>) -> u32 {
    let tail = dir
        .unwrap_or("")
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("");
    tail.chars()
        .rev()
        .take_while(|c: &char| c.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>()
        .parse()
        .unwrap_or(0)
}

/// Visible character width, ignoring ANSI escape sequences.
/// Counts emoji (>= U+1F000) as 2 wide.
fn display_width(s: &str) -> usize {
    let mut w = 0;
    let mut in_esc = false;
    for c in s.chars() {
        if in_esc {
            if c == 'm' {
                in_esc = false;
            }
            continue;
        }
        if c == '\x1b' {
            in_esc = true;
            continue;
        }
        let cp = c as u32;
        // Variant selectors (zero width)
        if cp == 0xFE0F || cp == 0xFE0E {
            continue;
        }
        // Enclosed Alphanumeric Supplement (🅾🆂🅷 etc.) — text presentation, 1 wide
        if (0x1F100..=0x1F1FF).contains(&cp) {
            w += 1;
        // Emoji (U+1F300+), BMP emoji-presentation symbols (❓ etc.) and CJK fullwidth → 2
        } else if cp >= 0x1F300 || (0x2E80..=0x9FFF).contains(&cp) || is_bmp_emoji_presentation(cp) {
            w += 2;
        } else {
            w += 1;
        }
    }
    w
}

// BMP code points with Emoji_Presentation=Yes (Unicode emoji-data.txt). Terminals
// draw these as 2-wide emoji even without U+FE0F. Text-presentation symbols
// (▲▼▓░①⧗ …) are deliberately absent: they stay 1 wide.
fn is_bmp_emoji_presentation(cp: u32) -> bool {
    matches!(
        cp,
        0x231A..=0x231B
            | 0x23E9..=0x23EC
            | 0x23F0
            | 0x23F3
            | 0x25FD..=0x25FE
            | 0x2614..=0x2615
            | 0x2648..=0x2653
            | 0x267F
            | 0x2693
            | 0x26A1
            | 0x26AA..=0x26AB
            | 0x26BD..=0x26BE
            | 0x26C4..=0x26C5
            | 0x26CE
            | 0x26D4
            | 0x26EA
            | 0x26F2..=0x26F3
            | 0x26F5
            | 0x26FA
            | 0x26FD
            | 0x2705
            | 0x270A..=0x270B
            | 0x2728
            | 0x274C
            | 0x274E
            | 0x2753..=0x2755
            | 0x2757
            | 0x2795..=0x2797
            | 0x27B0
            | 0x27BF
            | 0x2B1B..=0x2B1C
            | 0x2B50
            | 0x2B55
    )
}

fn pad_right(s: &str, width: usize) -> String {
    let w = display_width(s);
    if w >= width {
        s.to_string()
    } else {
        format!("{s}{}", " ".repeat(width - w))
    }
}

fn pad_left(s: &str, width: usize) -> String {
    let w = display_width(s);
    if w >= width {
        s.to_string()
    } else {
        format!("{}{s}", " ".repeat(width - w))
    }
}

// A cache with the same rate_limits younger than this is not rewritten.
// ycusage only labels data older than 3600s as stale.
const RATE_LIMITS_REWRITE_SECONDS: u64 = 300;

/// $CLAUDE_CONFIG_DIR/.rate-limits.json, read by bin/ycusage.ps1.
/// The rate_limits are re-serialized from the typed struct, so fields other
/// than five_hour/seven_day.{used_percentage,resets_at} are dropped, and
/// stdin whose rate_limits do not fit the struct (which also blanks the rest
/// of the Input) writes nothing. ycusage reads only those fields and
/// cached_at.
#[derive(Deserialize, Serialize)]
struct RateLimitsCache {
    rate_limits: RateLimits,
    cached_at: u64,
}

/// Whether the cache needs writing: skipped while the existing file holds
/// the same rate_limits written less than RATE_LIMITS_REWRITE_SECONDS ago.
fn rate_limits_cache_stale(existing: Option<&str>, rl: &RateLimits, now: u64) -> bool {
    let Some(prev) = existing.and_then(|s| serde_json::from_str::<RateLimitsCache>(s).ok()) else {
        return true;
    };
    let recent = prev.cached_at <= now && now - prev.cached_at < RATE_LIMITS_REWRITE_SECONDS;
    !(recent && prev.rate_limits == *rl)
}

/// Plain overwrite, not temp + rename: .rate-limits.json can be a symlink
/// (e.g. under ~/.claude-N.lock), and a rename would replace the link.
fn cache_rate_limits(dir: Option<&str>, rl: &RateLimits, now: u64) {
    let Some(dir) = dir else { return };
    let path = format!("{dir}/.rate-limits.json");
    let existing = std::fs::read_to_string(&path).ok();
    if !rate_limits_cache_stale(existing.as_deref(), rl, now) {
        return;
    }
    let cache = RateLimitsCache { rate_limits: rl.clone(), cached_at: now };
    if let Ok(json_str) = serde_json::to_string(&cache) {
        let _ = std::fs::write(&path, json_str);
    }
}

fn main() {
    // Claude Code が in-flight 実行をキャンセルした際、Windows では
    // 孫プロセスとして取り残されることがあり、stdin の EOF が来ず
    // read_to_string で永遠にブロックしてゾンビ化する。
    // 正常実行は数ms〜2秒(GLM API timeout=2s)なので 5秒で強制終了する。
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(5));
        std::process::exit(2);
    });

    let mut input_str = String::new();
    let _ = io::stdin().read_to_string(&mut input_str);
    let j: Input = serde_json::from_str(&input_str).unwrap_or_default();

    let dir = config_dir();
    let dir = dir.as_deref();

    let provider = detect_provider();
    let is_glm = provider.is_some();

    // Model
    let model_stdin = j.model.as_ref().and_then(|m| m.display_name.clone()).unwrap_or_default();
    let model_raw = if !model_stdin.is_empty() {
        model_stdin
    } else {
        j.transcript_path
            .as_deref()
            .and_then(read_model_from_transcript)
            .unwrap_or_else(|| "?".into())
    };

    let rules = if rules_needed(&model_raw, provider.as_deref()) {
        load_rules(dir)
    } else {
        RulesFile::default()
    };
    let provider_badge = provider.as_ref().and_then(|p| provider_badge_with_rules(p, &rules.provider))
        .unwrap_or_default();

    // Rate limits: GLM or Anthropic
    let rate_limits = if is_glm {
        Some(get_glm_rate_limits(dir))
    } else {
        if let Some(rl) = &j.rate_limits {
            cache_rate_limits(dir, rl, now_unix() as u64);
        }
        j.rate_limits.clone()
    };
    let has_rl = rate_limits.is_some();

    // Rate limits
    let (usage5h, elapsed5h, usage7d, elapsed7d) = if let Some(ref rl) = rate_limits {
        let u5 = rl.five_hour.as_ref().and_then(|w| w.used_percentage).unwrap_or(0.0).floor() as i32;
        let e5 = rl.five_hour.as_ref().map(|w| elapsed_pct(&w.resets_at, FIVE_HOURS)).unwrap_or(0);
        let u7 = rl.seven_day.as_ref().and_then(|w| w.used_percentage).unwrap_or(0.0).floor() as i32;
        let e7 = rl.seven_day.as_ref().map(|w| elapsed_pct(&w.resets_at, SEVEN_DAYS)).unwrap_or(0);
        (u5, e5, u7, e7)
    } else {
        (0, 0, 0, 0)
    };

    let model = model_short_with_rules(&model_raw, &rules.model);
    let acc = acc_from_dir(dir);
    let acc_str = if acc > 0 && (acc as usize) <= ACC_ICONS.len() {
        ACC_ICONS[(acc - 1) as usize].to_string()
    } else {
        String::new()
    };

    // Cost & lines
    let cost = j.cost.as_ref();
    let usd = cost.map(|c| c.total_cost_usd).unwrap_or(0.0);
    let added = cost.map(|c| c.total_lines_added).unwrap_or(0);
    let removed = cost.map(|c| c.total_lines_removed).unwrap_or(0);

    // Tokens
    let cu = j.context_window.as_ref().and_then(|c| c.current_usage.as_ref());
    let in_tok = cu.map(|u| u.input_tokens + u.cache_read_input_tokens + u.cache_creation_input_tokens).unwrap_or(0);
    let out_tok = cu.map(|u| u.output_tokens).unwrap_or(0);

    // Align +/▼ and -/▲ columns
    let add_str = format!("{GREEN}+{}{RST}", fmt_num(added));
    let in_str = format!("{CYAN}\u{25bc}{}{RST}", fmt_num(in_tok));
    let sub_str = format!("{RED}-{}{RST}", fmt_num(removed));
    let out_str = format!("{MAGENTA}\u{25b2}{}{RST}", fmt_num(out_tok));
    // Context window (normalized so the auto-compact threshold = 100%)
    let cx_stat = match j.context_window.as_ref().and_then(context_pct) {
        Some(p) => stat("Cx", p),
        None if has_rl => stat("Cx", 0),
        None => format!("Cx{LOADING}"),
    };

    // Sub-column widths for +/▼ and -/▲
    let aw = display_width(&add_str).max(display_width(&in_str));
    let sw = display_width(&sub_str).max(display_width(&out_str));

    // Effort
    let effort_str = j
        .effort
        .as_ref()
        .and_then(|e| e.level.as_deref())
        .map(effort_badge)
        .unwrap_or_default();

    // Line 1: model effort acc {gap} $cost | +add | -sub
    // Line 2: cx_stat                      | ▼in  | ▲out
    // Gap fills so that $cost right edge = cx_stat right edge
    let model_part = format!("{provider_badge}{model}{effort_str}{acc_str}");
    let cost_part = format!("{YELLOW}${usd:.1}{RST}");
    let model_w = display_width(&model_part);
    let cost_w = display_width(&cost_part);
    let cx_w = display_width(&cx_stat);
    let gap = if cx_w > model_w + cost_w + 1 {
        cx_w - model_w - cost_w
    } else {
        1
    };
    let prefix1 = format!("{model_part}{}{cost_part}", " ".repeat(gap));
    let pw = display_width(&prefix1).max(cx_w);

    // Build columns
    let col_l = [
        format!("{} {} {}", pad_right(&prefix1, pw), pad_right(&add_str, aw), pad_left(&sub_str, sw)),
        format!("{} {} {}", pad_right(&cx_stat, pw), pad_right(&in_str, aw), pad_left(&out_str, sw)),
    ];
    let col_c = [
        if has_rl { stat("5h", usage5h) } else { format!("5h{LOADING}") },
        if has_rl { stat("5t", elapsed5h) } else { format!("5t{LOADING}") },
    ];
    let col_r = [
        if has_rl { stat("7d", usage7d) } else { format!("7d{LOADING}") },
        if has_rl { stat("7t", elapsed7d) } else { format!("7t{LOADING}") },
    ];

    let pad_l = col_l.iter().map(|s| display_width(s)).max().unwrap_or(0);
    let pad_c = col_c.iter().map(|s| display_width(s)).max().unwrap_or(0);
    let pad_r = col_r.iter().map(|s| display_width(s)).max().unwrap_or(0);

    let mut out = String::new();
    for i in 0..2 {
        let _ = write!(
            out,
            "{}  {}  {}",
            pad_right(&col_l[i], pad_l),
            pad_right(&col_c[i], pad_c),
            pad_right(&col_r[i], pad_r),
        );
        if i == 0 {
            out.push('\n');
        }
    }
    print!("{out}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zai_cache(cached_at: u64, failed_at: u64) -> CachedRateLimits {
        CachedRateLimits {
            cached_at,
            provider: Some("z.ai".into()),
            rate_limits: Some(RateLimits::default()),
            failed_at,
        }
    }

    #[test]
    fn success_cache_fresh_until_ttl() {
        let now = 1_000_000;
        let c = zai_cache(now - (CACHE_TTL_SECONDS - 1), 0);
        assert!(cache_is_fresh(&c, now));
        let c = zai_cache(now - CACHE_TTL_SECONDS, 0);
        assert!(!cache_is_fresh(&c, now));
    }

    #[test]
    fn failure_cache_fresh_until_fail_ttl() {
        let now = 1_000_000;
        // last success long ago, recent failure
        let c = zai_cache(now - 3600, now - (FAIL_CACHE_TTL_SECONDS - 1));
        assert!(cache_is_fresh(&c, now));
        let c = zai_cache(now - 3600, now - FAIL_CACHE_TTL_SECONDS);
        assert!(!cache_is_fresh(&c, now));
        // never succeeded
        let c = zai_cache(0, now - 1);
        assert!(cache_is_fresh(&c, now));
        let c = zai_cache(0, now - FAIL_CACHE_TTL_SECONDS);
        assert!(!cache_is_fresh(&c, now));
    }

    #[test]
    fn non_zai_cache_is_never_fresh() {
        let now = 1_000_000;
        let mut c = zai_cache(now, now);
        c.provider = None;
        assert!(!cache_is_fresh(&c, now));
    }

    #[test]
    fn legacy_cache_without_failed_at_is_success() {
        let json = r#"{"cached_at":1000,"provider":"z.ai","rate_limits":{"five_hour":{"used_percentage":12.0,"resets_at":2000},"seven_day":null}}"#;
        let c: CachedRateLimits = serde_json::from_str(json).unwrap();
        assert_eq!(c.failed_at, 0);
        assert_eq!(c.cached_at, 1000);
        assert!(cache_is_fresh(&c, 1000 + CACHE_TTL_SECONDS - 1));
        assert!(!cache_is_fresh(&c, 1000 + CACHE_TTL_SECONDS));
    }

    #[test]
    fn failure_cache_keeps_data_and_skips_network_next_run() {
        let now = 1_000_000;
        let mut prev = zai_cache(now - 3600, 0);
        prev.rate_limits.as_mut().unwrap().five_hour = Some(Window {
            used_percentage: Some(42.0),
            resets_at: None,
        });
        let c = failure_cache(Some(prev), now);
        assert_eq!(c.cached_at, now - 3600);
        assert_eq!(c.failed_at, now);
        assert_eq!(c.provider.as_deref(), Some("z.ai"));
        assert_eq!(
            c.rate_limits.as_ref().and_then(|r| r.five_hour.as_ref()).and_then(|w| w.used_percentage),
            Some(42.0)
        );
        assert!(cache_is_fresh(&c, now + 1));
        assert!(!cache_is_fresh(&c, now + FAIL_CACHE_TTL_SECONDS));

        // no previous cache
        let c = failure_cache(None, now);
        assert_eq!(c.cached_at, 0);
        assert!(c.rate_limits.is_none());
        assert!(cache_is_fresh(&c, now + 1));

        // a previous cache that never succeeded has no data to keep
        let c = failure_cache(Some(zai_cache(0, now - 100)), now);
        assert!(c.rate_limits.is_none());
    }

    #[test]
    fn failure_cache_without_data_writes_null_rate_limits() {
        // ycusage treats a null rate_limits as no cache; an object with null
        // windows would show as 0% usage.
        let v = serde_json::to_value(failure_cache(None, 1_000)).unwrap();
        assert!(v.get("rate_limits").is_some_and(|r| r.is_null()));
        let back: CachedRateLimits = serde_json::from_value(v).unwrap();
        assert!(back.rate_limits.is_none());
        // a cache without the key also deserializes
        let c: CachedRateLimits =
            serde_json::from_str(r#"{"cached_at":0,"provider":"z.ai","failed_at":5}"#).unwrap();
        assert!(c.rate_limits.is_none());
    }

    #[test]
    fn fetch_failure_does_not_overwrite_unparsable_cache() {
        // a torn read of a good cache must not be replaced by an empty one
        let partial = br#"{"cached_at":1000,"provider":"z.ai","rate_li"#;
        let f = TempFile::new("glm-torn", partial);
        let rl = glm_fetch_failed(f.path(), GlmCacheRead::Unparsable, 2_000, "test");
        assert!(rl.five_hour.is_none() && rl.seven_day.is_none());
        assert_eq!(std::fs::read(f.path()).unwrap(), partial);

        // a missing cache gets a failure record with null rate_limits
        let f = TempFile::new("glm-missing", b"");
        std::fs::remove_file(f.path()).unwrap();
        glm_fetch_failed(f.path(), GlmCacheRead::Missing, 2_000, "test");
        let c: CachedRateLimits =
            serde_json::from_str(&std::fs::read_to_string(f.path()).unwrap()).unwrap();
        assert_eq!(c.failed_at, 2_000);
        assert!(c.rate_limits.is_none());
    }

    #[test]
    fn read_glm_cache_treats_only_recent_unparsable_file_as_torn() {
        let f = TempFile::new("glm-read", b"{\"cached_at\":1");
        assert!(matches!(read_glm_cache(f.path()), GlmCacheRead::Unparsable));

        let old = std::time::SystemTime::now() - TORN_WRITE_WINDOW * 2;
        std::fs::File::options().write(true).open(f.path()).unwrap().set_modified(old).unwrap();
        assert!(matches!(read_glm_cache(f.path()), GlmCacheRead::Missing));

        std::fs::write(f.path(), r#"{"cached_at":1,"provider":"z.ai","rate_limits":null}"#).unwrap();
        assert!(matches!(read_glm_cache(f.path()), GlmCacheRead::Parsed(_)));

        std::fs::remove_file(f.path()).unwrap();
        assert!(matches!(read_glm_cache(f.path()), GlmCacheRead::Missing));
    }

    struct TempFile(std::path::PathBuf);

    impl TempFile {
        fn new(name: &str, content: &[u8]) -> Self {
            let path = std::env::temp_dir()
                .join(format!("claude-statusline-test-{}-{name}.jsonl", std::process::id()));
            std::fs::write(&path, content).unwrap();
            TempFile(path)
        }
        fn path(&self) -> &str {
            self.0.to_str().unwrap()
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn assistant_line(model: &str) -> String {
        format!("{{\"type\":\"assistant\",\"message\":{{\"model\":\"{model}\",\"content\":[]}}}}\n")
    }

    #[test]
    fn transcript_skips_partial_last_line() {
        let content = format!(
            "{{\"type\":\"user\"}}\n{}{{\"type\":\"assistant\",\"mess",
            assistant_line("glm-4.6")
        );
        let f = TempFile::new("partial", content.as_bytes());
        assert_eq!(read_model_from_transcript(f.path()).as_deref(), Some("glm-4.6"));
    }

    #[test]
    fn transcript_skips_assistant_line_without_model() {
        let content = format!(
            "{}{{\"type\":\"assistant\",\"message\":{{\"content\":[]}}}}\n{{\"type\":\"assistant\"}}\n",
            assistant_line("glm-4.5")
        );
        let f = TempFile::new("nomodel", content.as_bytes());
        assert_eq!(read_model_from_transcript(f.path()).as_deref(), Some("glm-4.5"));
    }

    #[test]
    fn transcript_tolerates_invalid_utf8() {
        let mut content = b"\xff\xfe{\"type\":\"user\",\"x\":\"\xc3\"}\n".to_vec();
        content.extend_from_slice(assistant_line("glm-4.6").as_bytes());
        let f = TempFile::new("utf8", &content);
        assert_eq!(read_model_from_transcript(f.path()).as_deref(), Some("glm-4.6"));
    }

    #[test]
    fn transcript_larger_than_window_returns_tail_model() {
        let filler = "{\"type\":\"user\",\"message\":\"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\"}\n";
        let mut content = assistant_line("head-model");
        for _ in 0..12_000 {
            content.push_str(filler);
        }
        content.push_str(&assistant_line("tail-model"));
        content.push_str(filler);
        assert!(content.len() as u64 > TRANSCRIPT_TAIL_INITIAL);
        let f = TempFile::new("large", content.as_bytes());
        assert_eq!(read_model_from_transcript(f.path()).as_deref(), Some("tail-model"));
    }

    #[test]
    fn transcript_window_doubles_until_cap() {
        let filler = "{\"type\":\"user\",\"message\":\"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\"}\n";
        let mut content = assistant_line("near-model");
        for _ in 0..10 {
            content.push_str(filler);
        }
        let f = TempFile::new("double", content.as_bytes());
        // initial window 64B misses the model; doubling reaches it
        assert_eq!(
            read_model_from_transcript_tail(f.path(), 64, 4096).as_deref(),
            Some("near-model")
        );
        // cap below the model's offset: not found
        assert_eq!(read_model_from_transcript_tail(f.path(), 64, 256), None);
    }

    #[test]
    fn transcript_line_at_window_edge_is_kept() {
        let line = assistant_line("edge-model");
        let content = format!("{{\"type\":\"user\"}}\n{line}");
        let f = TempFile::new("edge", content.as_bytes());
        let window = line.len() as u64;
        assert_eq!(
            read_model_from_transcript_tail(f.path(), window, window).as_deref(),
            Some("edge-model")
        );
    }

    #[test]
    fn transcript_missing_or_empty_file() {
        assert_eq!(read_model_from_transcript("Z:/nonexistent/claude-statusline.jsonl"), None);
        let f = TempFile::new("empty", b"");
        assert_eq!(read_model_from_transcript(f.path()), None);
    }

    #[test]
    fn cache_json_keeps_key_names() {
        let v = serde_json::to_value(zai_cache(5, 0)).unwrap();
        for key in ["cached_at", "provider", "rate_limits", "failed_at"] {
            assert!(v.get(key).is_some(), "missing {key}");
        }
    }

    fn env_of<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| vars.iter().find(|(k, _)| *k == name).map(|(_, v)| v.to_string())
    }

    #[test]
    fn config_dir_prefers_claude_config_dir() {
        let vars = [("CLAUDE_CONFIG_DIR", "C:/u/.claude-3"), ("USERPROFILE", "C:/u"), ("HOME", "/h")];
        assert_eq!(config_dir_from(env_of(&vars)).as_deref(), Some("C:/u/.claude-3"));
    }

    #[test]
    fn config_dir_treats_empty_as_unset() {
        let vars = [("CLAUDE_CONFIG_DIR", ""), ("USERPROFILE", "C:/u"), ("HOME", "/h")];
        assert_eq!(config_dir_from(env_of(&vars)).as_deref(), Some("C:/u/.claude"));
        let vars = [("CLAUDE_CONFIG_DIR", ""), ("USERPROFILE", ""), ("HOME", "/h")];
        assert_eq!(config_dir_from(env_of(&vars)).as_deref(), Some("/h/.claude"));
        let vars = [("CLAUDE_CONFIG_DIR", ""), ("USERPROFILE", ""), ("HOME", "")];
        assert_eq!(config_dir_from(env_of(&vars)), None);
    }

    #[test]
    fn config_dir_falls_back_userprofile_then_home() {
        let vars = [("USERPROFILE", "C:/u"), ("HOME", "/h")];
        assert_eq!(config_dir_from(env_of(&vars)).as_deref(), Some("C:/u/.claude"));
        let vars = [("HOME", "/h")];
        assert_eq!(config_dir_from(env_of(&vars)).as_deref(), Some("/h/.claude"));
        assert_eq!(config_dir_from(env_of(&[])), None);
    }

    #[test]
    fn acc_from_dir_reads_trailing_digits() {
        assert_eq!(acc_from_dir(Some("C:/u/.claude-3")), 3);
        assert_eq!(acc_from_dir(Some("C:\\u\\.claude-9\\")), 9);
        assert_eq!(acc_from_dir(Some("/h/.claude-12/")), 12);
        // every fallback ends in /.claude, whichever of USERPROFILE/HOME is used
        assert_eq!(acc_from_dir(Some("C:/u3/.claude")), 0);
        assert_eq!(acc_from_dir(Some("/h3/.claude")), 0);
        assert_eq!(acc_from_dir(None), 0);
    }

    fn rule(names: &[&str], display: &str) -> Rule {
        Rule {
            name: names.iter().map(|s| s.to_string()).collect(),
            display: display.into(),
            color: "1,2,3".into(),
        }
    }

    #[test]
    fn match_rule_is_case_insensitive_and_first_wins() {
        let rules = [rule(&["glm-4.6", "GLM-4.6-Air"], "a"), rule(&["glm-4.6"], "b")];
        assert_eq!(match_rule("GLM-4.6", &rules).map(|r| r.display.as_str()), Some("a"));
        assert_eq!(match_rule("glm-4.6-air", &rules).map(|r| r.display.as_str()), Some("a"));
        assert!(match_rule("glm-4", &rules).is_none());
        assert!(match_rule("x", &[]).is_none());
    }

    #[test]
    fn bundled_rules_parse() {
        let parsed: RulesFile = toml::from_str(include_str!("../statusline-models.toml")).unwrap();
        assert!(!parsed.model.is_empty() || !parsed.provider.is_empty());
    }

    #[test]
    fn rules_not_needed_for_anthropic_models() {
        assert!(!rules_needed("Opus 5.5", None));
        assert!(!rules_needed("Sonnet 4.6 (1M context)", None));
        assert!(!rules_needed("Haiku 4.5", None));
        // no version: anthropic_badge gives None, rules decide
        assert!(rules_needed("Opus", None));
        assert!(rules_needed("glm-4.6", None));
        assert!(rules_needed("?", None));
        // provider badge always needs the rules
        assert!(rules_needed("Opus 5.5", Some("z.ai")));
    }

    fn rl(pct5: f64) -> RateLimits {
        RateLimits {
            five_hour: Some(Window {
                used_percentage: Some(pct5),
                resets_at: Some(serde_json::json!(1791699600)),
            }),
            seven_day: Some(Window {
                used_percentage: Some(22.0),
                resets_at: Some(serde_json::json!(1792180800)),
            }),
        }
    }

    fn rl_cache_json(r: &RateLimits, cached_at: u64) -> String {
        serde_json::to_string(&RateLimitsCache { rate_limits: r.clone(), cached_at }).unwrap()
    }

    #[test]
    fn rate_limits_cache_skips_same_recent() {
        let now = 1_000_000;
        let same = rl_cache_json(&rl(23.0), now - (RATE_LIMITS_REWRITE_SECONDS - 1));
        assert!(!rate_limits_cache_stale(Some(&same), &rl(23.0), now));
        // too old
        let old = rl_cache_json(&rl(23.0), now - RATE_LIMITS_REWRITE_SECONDS);
        assert!(rate_limits_cache_stale(Some(&old), &rl(23.0), now));
        // changed
        assert!(rate_limits_cache_stale(Some(&same), &rl(24.0), now));
        // written by a clock ahead of ours
        let future = rl_cache_json(&rl(23.0), now + 10);
        assert!(rate_limits_cache_stale(Some(&future), &rl(23.0), now));
        // missing or unreadable
        assert!(rate_limits_cache_stale(None, &rl(23.0), now));
        assert!(rate_limits_cache_stale(Some("not json"), &rl(23.0), now));
    }

    #[test]
    fn rate_limits_cache_compares_values_not_text() {
        // written by the raw-copy version: integers and extra fields
        let now = 1_000_000;
        let legacy = format!(
            r#"{{"cached_at":{},"rate_limits":{{"five_hour":{{"used_percentage":23,"resets_at":1791699600}},"seven_day":{{"used_percentage":22,"resets_at":1792180800}},"extra":1}}}}"#,
            now - 5
        );
        assert!(!rate_limits_cache_stale(Some(&legacy), &rl(23.0), now));
    }

    #[test]
    fn cache_rate_limits_writes_only_when_stale() {
        let dir = std::env::temp_dir()
            .join(format!("claude-statusline-test-{}-rlcache", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let d = dir.to_str().unwrap();
        let path = dir.join(".rate-limits.json");
        let read = || serde_json::from_str::<RateLimitsCache>(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let now = 1_000_000;

        cache_rate_limits(Some(d), &rl(23.0), now);
        assert_eq!(read().cached_at, now);
        // same data shortly after: file left as is
        cache_rate_limits(Some(d), &rl(23.0), now + 10);
        assert_eq!(read().cached_at, now);
        // changed data: rewritten
        cache_rate_limits(Some(d), &rl(30.0), now + 20);
        let c = read();
        assert_eq!(c.cached_at, now + 20);
        assert_eq!(c.rate_limits, rl(30.0));
        // same data after the rewrite interval: rewritten
        cache_rate_limits(Some(d), &rl(30.0), now + 20 + RATE_LIMITS_REWRITE_SECONDS);
        assert_eq!(read().cached_at, now + 20 + RATE_LIMITS_REWRITE_SECONDS);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn display_width_counts_bmp_emoji_as_wide() {
        assert_eq!(display_width("❓"), 2);
        assert_eq!(display_width("❓ foo"), 6);
        assert_eq!(display_width("\x1b[38;2;150;150;150m❓ ?\x1b[0m"), 4);
        assert_eq!(display_width("⚡✅⭐❔❕❗"), 12);
        // U+FE0F after a wide emoji adds nothing
        assert_eq!(display_width("❓\u{fe0f}"), 2);
    }

    #[test]
    fn display_width_keeps_text_symbols_narrow() {
        assert_eq!(display_width("🅾"), 1);
        assert_eq!(display_width("🅶"), 1);
        assert_eq!(display_width("①⑨"), 2);
        assert_eq!(display_width("▓░▼▲"), 4);
        assert_eq!(display_width("⧗"), 1);
        // emoji with text presentation by default (Emoji_Presentation=No)
        assert_eq!(display_width("✔❤⚠"), 3);
        assert_eq!(display_width("漢字かな"), 8);
    }
}
