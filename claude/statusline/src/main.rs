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

#[derive(Deserialize, Serialize, Clone, Default)]
struct RateLimits {
    five_hour: Option<Window>,
    seven_day: Option<Window>,
}

#[derive(Deserialize, Serialize, Clone, Default)]
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

fn read_model_from_transcript(path: &str) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    for line in content.lines().rev() {
        if !line.contains("\"type\"") || !line.contains("\"assistant\"") {
            continue;
        }
        let obj: serde_json::Value = serde_json::from_str(line).ok()?;
        if obj.get("type")?.as_str()? == "assistant" {
            return obj
                .get("message")?
                .get("model")?
                .as_str()
                .map(|s| s.to_string());
        }
    }
    None
}

// ========================================
// Statusline TOML Rules
// ========================================

#[derive(Deserialize, Default, Clone)]
struct ModelRule {
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
    model: Vec<ModelRule>,
    #[serde(default)]
    provider: Vec<ProviderRule>,
}

#[derive(Deserialize, Default, Clone)]
struct ProviderRule {
    #[serde(default)]
    name: Vec<String>,
    #[serde(default)]
    display: String,
    #[serde(default)]
    color: String,
}

// ========================================
// TOML Rule Loading
// ========================================

fn match_model_rule<'a>(model_name: &'a str, rules: &'a [ModelRule]) -> Option<&'a ModelRule> {
    rules.iter().find(|rule| {
        rule.name.iter().any(|n| n.eq_ignore_ascii_case(model_name))
    })
}

struct LoadedRules {
    model: Vec<ModelRule>,
    provider: Vec<ProviderRule>,
}

fn load_rules() -> LoadedRules {
    let dir = env::var("CLAUDE_CONFIG_DIR").unwrap_or_else(|_| {
        env::var("USERPROFILE")
            .or_else(|_| env::var("HOME"))
            .map(|h| format!("{}/.claude", h))
            .unwrap_or_default()
    });
    let rules_path = format!("{dir}/statusline-rules.toml");
    let content = std::fs::read_to_string(&rules_path)
        .unwrap_or_else(|_| include_str!("../statusline-models.toml").to_string());
    let parsed = toml::from_str::<RulesFile>(&content)
        .unwrap_or_else(|e| {
            eprintln!("Failed to parse statusline rules: {e}");
            RulesFile::default()
        });
    LoadedRules {
        model: parsed.model,
        provider: parsed.provider,
    }
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

fn model_short_with_rules(display_name: &str, rules: &[ModelRule]) -> String {
    // Anthropic モデルはパターン解析で自動生成（toml 追記不要）
    if let Some((disp, color)) = anthropic_badge(display_name) {
        return format!("{}{disp}{RST}", fmt_color(&color));
    }
    if let Some(rule) = match_model_rule(display_name, rules) {
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

fn match_provider_rule<'a>(name: &str, rules: &'a [ProviderRule]) -> Option<&'a ProviderRule> {
    rules.iter().find(|r| r.name.iter().any(|n| n.eq_ignore_ascii_case(name)))
}

fn detect_provider() -> Option<String> {
    env::var("ANTHROPIC_BASE_URL")
        .ok()
        .filter(|url| url.to_lowercase().contains("z.ai"))
        .map(|_| "z.ai".into())
}

fn provider_badge_with_rules(name: &str, rules: &[ProviderRule]) -> Option<String> {
    match_provider_rule(name, rules).map(|rule| fmt_badge(&rule.display, &rule.color))
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

fn get_glm_rate_limits() -> RateLimits {
    let dir = env::var("CLAUDE_CONFIG_DIR").unwrap_or_else(|_| {
        env::var("USERPROFILE")
            .or_else(|_| env::var("HOME"))
            .map(|h| format!("{h}/.claude"))
            .unwrap_or_default()
    });
    if dir.is_empty() {
        return RateLimits::default();
    }
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

fn acc_from_env() -> u32 {
    let dir = env::var("CLAUDE_CONFIG_DIR").unwrap_or_default();
    let dir = if dir.is_empty() {
        env::var("HOME")
            .or_else(|_| env::var("USERPROFILE"))
            .map(|h| format!("{h}/.claude"))
            .unwrap_or_default()
    } else {
        dir
    };
    let tail = dir
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
        // Emoji (U+1F300+) and CJK fullwidth → 2
        } else if cp >= 0x1F300 || (0x2E80..=0x9FFF).contains(&cp) {
            w += 2;
        } else {
            w += 1;
        }
    }
    w
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

/// Cache rate_limits to $CLAUDE_CONFIG_DIR/.rate-limits.json for ycusage
fn cache_rate_limits(raw: &str) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) else {
        return;
    };
    let Some(rl) = v.get("rate_limits") else { return };
    if rl.is_null() {
        return;
    }
    let dir = env::var("CLAUDE_CONFIG_DIR").unwrap_or_else(|_| {
        env::var("USERPROFILE")
            .or_else(|_| env::var("HOME"))
            .map(|h| format!("{h}/.claude"))
            .unwrap_or_default()
    });
    if dir.is_empty() {
        return;
    }
    let cache = serde_json::json!({
        "rate_limits": rl,
        "cached_at": now_unix() as u64,
    });
    let _ = std::fs::write(format!("{dir}/.rate-limits.json"), cache.to_string());
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

    let LoadedRules { model: rules, provider: provider_rules } = load_rules();

    let provider = detect_provider();
    let is_glm = provider.is_some();
    let provider_badge = provider.as_ref().and_then(|p| provider_badge_with_rules(p, &provider_rules))
        .unwrap_or_default();

    // Rate limits: GLM or Anthropic
    let rate_limits = if is_glm {
        Some(get_glm_rate_limits())
    } else {
        cache_rate_limits(&input_str);
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
    let model = model_short_with_rules(&model_raw, &rules);
    let acc = acc_from_env();
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

    #[test]
    fn cache_json_keeps_key_names() {
        let v = serde_json::to_value(zai_cache(5, 0)).unwrap();
        for key in ["cached_at", "provider", "rate_limits", "failed_at"] {
            assert!(v.get(key).is_some(), "missing {key}");
        }
    }
}
