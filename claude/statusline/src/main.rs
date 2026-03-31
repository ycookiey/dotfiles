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
}

#[derive(Deserialize, Default)]
struct Model {
    display_name: Option<String>,
}

#[derive(Deserialize, Default)]
struct ContextWindow {
    used_percentage: Option<f64>,
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
    format!("\x1b[38;2;{}m", rgb)
}

/// Badge (1-glyph icon): trailing space to prevent glyph overlap in terminals
fn fmt_badge(display: &str, color: &str) -> String {
    format!("{}{display}{RST} ", fmt_color(color))
}

fn model_short_with_rules(display_name: &str, rules: &[ModelRule]) -> String {
    if let Some(rule) = match_model_rule(display_name, rules) {
        let color = fmt_color(&rule.color);
        format!("{color}{}{RST}", rule.display)
    } else {
        // 未知モデル
        format!("\x1b[38;2;150;150;150m❓ {display_name}\x1b[0m")
    }
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

#[derive(Deserialize, Serialize, Default)]
struct CachedRateLimits {
    cached_at: u64,
    provider: Option<String>,
    rate_limits: RateLimits,
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
    let parsed_cache: Option<CachedRateLimits> = std::fs::read_to_string(&cache_path)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok());

    // Check if cache is fresh AND is from z.ai
    if let Some(ref cached_data) = parsed_cache {
        let age = now.saturating_sub(cached_data.cached_at);
        let is_zai = cached_data.provider.as_deref() == Some("z.ai");
        if age < CACHE_TTL_SECONDS && is_zai {
            return cached_data.rate_limits.clone();
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
        .timeout(std::time::Duration::from_secs(3))
        .call() {
        Ok(r) => r,
        Err(e) => {
            // API failed: use stale cache if available
            eprintln!("GLM API request failed: {e}");
            return parsed_cache.unwrap_or_default().rate_limits;
        }
    };

    let body: serde_json::Value = match resp.into_string() {
        Ok(s) => match serde_json::from_str(&s) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Failed to parse GLM JSON: {e}, body: {s}");
                return parsed_cache.unwrap_or_default().rate_limits;
            }
        },
        Err(e) => {
            eprintln!("Failed to read GLM response body: {e}");
            return parsed_cache.unwrap_or_default().rate_limits;
        }
    };

    let limits_vec: Vec<serde_json::Value> = body.get("data")
        .and_then(|d| d.get("limits"))
        .and_then(|l| l.as_array())
        .map(|v| v.to_vec())
        .unwrap_or_default();
    let limits = limits_vec.as_slice();

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

    eprintln!("GLM parsed: five_hour_pct={}, seven_day_pct={}, five_hour_reset={:?}, seven_day_reset={:?}",
        five_hour_pct, seven_day_pct, five_hour_reset, seven_day_reset);

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
        rate_limits: rl.clone(),
    };
    let json_str = serde_json::to_string(&cache_data).unwrap_or_default();
    let _ = std::fs::write(&cache_path, json_str);

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
    // Context window
    let cx_stat = match j.context_window.as_ref().and_then(|c| c.used_percentage) {
        Some(p) => stat("Cx", p as i32),
        None if has_rl => stat("Cx", 0),
        None => format!("Cx{LOADING}"),
    };

    // Sub-column widths for +/▼ and -/▲
    let aw = display_width(&add_str).max(display_width(&in_str));
    let sw = display_width(&sub_str).max(display_width(&out_str));

    // Line 1: model {gap} $cost | +add | -sub
    // Line 2: cx_stat           | ▼in  | ▲out
    // Gap fills so that $cost right edge = cx_stat right edge
    let model_part = format!("{provider_badge}{model}{acc_str}");
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
