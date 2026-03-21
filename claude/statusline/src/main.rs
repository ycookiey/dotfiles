// Claude Code statusline: 2-line, 3-column (Model/Acc, 5h/7d usage + elapsed)
use serde::Deserialize;
use std::io::{self, Read};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, fmt::Write};

const BAR_WIDTH: usize = 6;
const FIVE_HOURS: f64 = 18000.0;
const SEVEN_DAYS: f64 = 604800.0;
const LOADING: &str = "             \u{29d7}"; // 6 + 4 spaces + ⧗ (= same width as bar)

#[derive(Deserialize, Default)]
struct Input {
    model: Option<Model>,
    context_window: Option<ContextWindow>,
    rate_limits: Option<RateLimits>,
    transcript_path: Option<String>,
}

#[derive(Deserialize, Default)]
struct Model {
    display_name: Option<String>,
}

#[derive(Deserialize, Default)]
struct ContextWindow {
    used_percentage: Option<f64>,
}

#[derive(Deserialize, Default)]
struct RateLimits {
    five_hour: Option<Window>,
    seven_day: Option<Window>,
}

#[derive(Deserialize, Default)]
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
    format!("{}{} {:3}%", label, bar(p), p)
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
        serde_json::Value::String(s) => {
            // ISO 8601 → unix timestamp (minimal parse)
            // Format: 2026-03-20T15:00:00Z or with offset
            parse_iso8601(s)
        }
        _ => None,
    }
}

fn parse_iso8601(s: &str) -> Option<f64> {
    // Minimal ISO 8601 parser: YYYY-MM-DDTHH:MM:SSZ
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

    // Days from epoch (1970-01-01) using a simplified calculation
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
                .map(model_display_name);
        }
    }
    None
}

fn model_display_name(id: &str) -> String {
    match id {
        s if s.contains("opus") => {
            if s.contains("4-6") {
                "Opus 4.6".into()
            } else {
                "Opus".into()
            }
        }
        s if s.contains("sonnet") => {
            if s.contains("4-6") {
                "Sonnet 4.6".into()
            } else {
                "Sonnet".into()
            }
        }
        s if s.contains("haiku") => "Haiku 4.5".into(),
        other => other.to_string(),
    }
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

fn main() {
    let mut input_str = String::new();
    let _ = io::stdin().read_to_string(&mut input_str);
    let j: Input = serde_json::from_str(&input_str).unwrap_or_default();

    let acc = acc_from_env();
    let has_rl = j.rate_limits.is_some();

    // Rate limits
    let (usage5h, elapsed5h, usage7d, elapsed7d) = if let Some(ref rl) = j.rate_limits {
        let u5 = rl
            .five_hour
            .as_ref()
            .and_then(|w| w.used_percentage)
            .unwrap_or(0.0)
            .floor() as i32;
        let e5 = rl
            .five_hour
            .as_ref()
            .map(|w| elapsed_pct(&w.resets_at, FIVE_HOURS))
            .unwrap_or(0);
        let u7 = rl
            .seven_day
            .as_ref()
            .and_then(|w| w.used_percentage)
            .unwrap_or(0.0)
            .floor() as i32;
        let e7 = rl
            .seven_day
            .as_ref()
            .map(|w| elapsed_pct(&w.resets_at, SEVEN_DAYS))
            .unwrap_or(0);
        (u5, e5, u7, e7)
    } else {
        (0, 0, 0, 0)
    };

    // Model: prefer stdin, fallback to transcript
    let model_stdin = j
        .model
        .as_ref()
        .and_then(|m| m.display_name.clone())
        .unwrap_or_default();
    let model = if !model_stdin.is_empty() {
        // TODO: transcript fallback for cross-session model bug
        model_stdin
    } else {
        j.transcript_path
            .as_deref()
            .and_then(read_model_from_transcript)
            .unwrap_or_else(|| "?".into())
    };

    // Context window
    let cx = match j.context_window.as_ref().and_then(|c| c.used_percentage) {
        Some(p) => stat("Cx", p as i32),
        None if has_rl => stat("Cx", 0),
        None => format!("Cx{LOADING}"),
    };

    let col_l = [format!("{model} Acc:{acc}"), cx];
    let col_c = [
        if has_rl { stat("5h", usage5h) } else { format!("5h{LOADING}") },
        if has_rl { stat("5t", elapsed5h) } else { format!("5t{LOADING}") },
    ];
    let col_r = [
        if has_rl { stat("7d", usage7d) } else { format!("7d{LOADING}") },
        if has_rl { stat("7t", elapsed7d) } else { format!("7t{LOADING}") },
    ];

    fn display_width(s: &str) -> usize {
        s.chars().count()
    }
    fn pad_right(s: &str, width: usize) -> String {
        let w = display_width(s);
        if w >= width { s.to_string() } else { format!("{s}{}", " ".repeat(width - w)) }
    }

    let pad_l = col_l.iter().map(|s| display_width(s)).max().unwrap_or(0);
    let pad_c = col_c.iter().map(|s| display_width(s)).max().unwrap_or(0);
    let pad_r = col_r.iter().map(|s| display_width(s)).max().unwrap_or(0);

    let mut out = String::new();
    for i in 0..2 {
        let _ = write!(
            out,
            "{}   {}   {}",
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
