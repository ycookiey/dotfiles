use std::io::Read;
use rusqlite::TransactionBehavior;
use crate::commands::{cas_db, cas_hash};

pub fn run() {
    if let Some(output) = try_run() {
        println!("{}", output);
    }
}

fn try_run() -> Option<String> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).ok()?;
    let payload: serde_json::Value = serde_json::from_str(&input).ok()?;

    let tool_name = payload["tool_name"].as_str()?;
    let file_path = extract_file_path(&payload, tool_name)?;
    let session_id = payload["session_id"].as_str().filter(|s| !s.is_empty())?;

    let normalized_path = cas_hash::normalize_path(&file_path);

    let db_path = cas_db::cas_db_path()?;
    let mut conn = cas_db::open_db(&db_path).ok()?;
    cas_db::migrate(&mut conn).ok()?;

    let file_content = std::fs::read(&file_path).ok()?;
    let current_blob = cas_hash::compute_blob_hash(&file_content);

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).ok()?;

    let entry = cas_db::lookup_entry(&*tx, &normalized_path, session_id).ok()?;

    let warning = match entry {
        None => {
            let head_blob = cas_hash::head_blob_hash_from_git(std::path::Path::new(&file_path));
            cas_db::insert_entry(&*tx, &normalized_path, session_id, head_blob.as_deref(), Some(&current_blob), None).ok()?;

            if head_blob.is_none() {
                None
            } else if head_blob.as_deref() == Some(current_blob.as_str()) {
                None
            } else {
                Some("This file has uncommitted changes — editing may mix unrelated changes into a single commit. Consider committing first.".to_string())
            }
        }
        Some(e) => {
            let warn = if e.last_seen.as_deref() == Some(current_blob.as_str()) {
                None
            } else if e.last_written.as_deref() == Some(current_blob.as_str()) {
                None
            } else {
                Some("External changes detected since Claude's last edit. Review before proceeding to avoid mixing unrelated changes.".to_string())
            };
            cas_db::update_last_seen(&*tx, &normalized_path, session_id, &current_blob).ok()?;
            warn
        }
    };

    tx.commit().ok()?;

    warning.map(|msg| {
        serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "allow",
                "additionalContext": msg
            }
        }).to_string()
    })
}

fn extract_file_path(payload: &serde_json::Value, tool_name: &str) -> Option<String> {
    match tool_name {
        "NotebookEdit" => payload["tool_input"]["notebook_path"].as_str().map(String::from),
        _ => payload["tool_input"]["file_path"].as_str().map(String::from),
    }
}
