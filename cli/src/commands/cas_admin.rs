use crate::commands::cas_db;

pub fn gc(days: u32) {
    let Some(db_path) = cas_db::cas_db_path() else {
        eprintln!("cas: DB path not resolvable");
        return;
    };
    let Ok(mut conn) = cas_db::open_db(&db_path) else {
        eprintln!("cas: DB open failed");
        return;
    };
    let _ = cas_db::migrate(&mut conn);
    let cutoff = cas_db::epoch_now() - (days as i64) * 86400;
    match conn.execute("DELETE FROM cas_journal WHERE updated_at < ?1", rusqlite::params![cutoff]) {
        Ok(n) => eprintln!("cas gc: deleted {} entries (older than {} days)", n, days),
        Err(e) => eprintln!("cas gc: failed: {}", e),
    }
}

pub fn inspect(session: Option<&str>, path: Option<&str>) {
    let Some(db_path) = cas_db::cas_db_path() else {
        eprintln!("cas: DB path not resolvable");
        return;
    };
    let Ok(mut conn) = cas_db::open_db(&db_path) else {
        eprintln!("cas: DB open failed");
        return;
    };
    let _ = cas_db::migrate(&mut conn);

    println!("path\tsession_id\thead_blob\tlast_seen\tlast_written\tupdated_at");

    let base = "SELECT path, session_id, head_blob, last_seen, last_written, updated_at FROM cas_journal";

    macro_rules! print_stmt {
        ($stmt:expr, $params:expr) => {
            if let Ok(rows) = $stmt.query_map($params, |row| {
                let p: String = row.get(0)?;
                let sid: String = row.get(1)?;
                let head: Option<String> = row.get(2)?;
                let seen: Option<String> = row.get(3)?;
                let written: Option<String> = row.get(4)?;
                let updated: i64 = row.get(5)?;
                Ok((p, sid, head, seen, written, updated))
            }) {
                for r in rows.flatten() {
                    println!("{}\t{}\t{}\t{}\t{}\t{}",
                        r.0, r.1,
                        r.2.as_deref().unwrap_or("-"),
                        r.3.as_deref().unwrap_or("-"),
                        r.4.as_deref().unwrap_or("-"),
                        r.5,
                    );
                }
            }
        };
    }

    match (session, path) {
        (None, None) => {
            let sql = format!("{base} ORDER BY updated_at DESC");
            if let Ok(mut stmt) = conn.prepare(&sql) {
                print_stmt!(stmt, []);
            }
        }
        (Some(s), None) => {
            let sql = format!("{base} WHERE session_id = ?1 ORDER BY updated_at DESC");
            if let Ok(mut stmt) = conn.prepare(&sql) {
                print_stmt!(stmt, rusqlite::params![s]);
            }
        }
        (None, Some(p)) => {
            let sql = format!("{base} WHERE path = ?1 ORDER BY updated_at DESC");
            if let Ok(mut stmt) = conn.prepare(&sql) {
                print_stmt!(stmt, rusqlite::params![p]);
            }
        }
        (Some(s), Some(p)) => {
            let sql = format!("{base} WHERE session_id = ?1 AND path = ?2 ORDER BY updated_at DESC");
            if let Ok(mut stmt) = conn.prepare(&sql) {
                print_stmt!(stmt, rusqlite::params![s, p]);
            }
        }
    }
}
