/// PostToolUse hook: log tool calls for token audit.
/// Replaces bash token-audit-log.sh to reduce process spawn overhead.
use std::io::{self, Read};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn run() {
    // Gate on env var — silent no-op when disabled
    if std::env::var("TOKEN_AUDIT_LOG").as_deref() != Ok("1") {
        return;
    }

    // All errors are silently ignored; a hook must never crash Claude Code.
    let _ = try_run();
}

fn try_run() -> io::Result<()> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;

    let val: serde_json::Value = serde_json::from_str(&input)?;

    let tool_name = val["tool_name"].as_str().unwrap_or("unknown");
    let session_id = val["session_id"].as_str().unwrap_or("unknown");
    let result_size = val
        .get("tool_response")
        .map(|v| v.to_string().len())
        .unwrap_or(0);

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let home = dirs::home_dir().ok_or_else(|| io::Error::other("no home dir"))?;
    let log_path = home.join(".claude/token-audit.log");

    let line = format!("{ts}\t{tool_name}\t{session_id}\t{result_size}\n");

    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    f.write_all(line.as_bytes())?;

    Ok(())
}
