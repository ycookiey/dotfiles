use std::io::Read;
use crate::commands::{cas_db, cas_hash};

pub fn run(failure: bool) {
    let _ = try_run(failure);
}

fn try_run(failure: bool) -> Option<()> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).ok()?;
    let payload: serde_json::Value = serde_json::from_str(&input).ok()?;

    let tool_name = payload["tool_name"].as_str().unwrap_or("");
    let file_path = match tool_name {
        "NotebookEdit" => payload["tool_input"]["notebook_path"].as_str(),
        _ => payload["tool_input"]["file_path"].as_str(),
    }?.to_string();

    let session_id = payload["session_id"].as_str().filter(|s| !s.is_empty())?;

    let normalized_path = cas_hash::normalize_path(&file_path);

    let file_content = std::fs::read(&file_path).ok()?;
    let current_blob = cas_hash::compute_blob_hash(&file_content);

    let db_path = cas_db::cas_db_path()?;
    let mut conn = cas_db::open_db(&db_path).ok()?;
    cas_db::migrate(&mut conn).ok()?;

    let head_blob = cas_hash::head_blob_hash_from_git(std::path::Path::new(&file_path));

    if failure {
        cas_db::upsert_seen_only(&conn, &normalized_path, session_id, head_blob.as_deref(), &current_blob).ok()?;
    } else {
        cas_db::upsert_seen_and_written(&conn, &normalized_path, session_id, head_blob.as_deref(), &current_blob).ok()?;
    }

    cas_db::probabilistic_gc(&conn);

    Some(())
}
