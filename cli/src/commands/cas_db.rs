use rusqlite::{Connection, params};
use rusqlite_migration::{M, Migrations};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn open_db(db_path: &Path) -> rusqlite::Result<Connection> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let conn = Connection::open(db_path)?;
    conn.execute_batch("
        PRAGMA journal_mode = WAL;
        PRAGMA busy_timeout = 2000;
        PRAGMA synchronous = NORMAL;
    ")?;
    Ok(conn)
}

pub fn migrate(conn: &mut Connection) -> anyhow::Result<()> {
    let migrations = Migrations::new(vec![
        M::up(include_str!("migrations/001_initial.sql")),
    ]);
    migrations.to_latest(conn)?;
    Ok(())
}

pub fn cas_db_path() -> Option<PathBuf> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok()?;
    Some(PathBuf::from(home).join(".claude").join("db").join("cas-journal.db"))
}

#[allow(dead_code)]
pub struct CasEntry {
    pub path: String,
    pub session_id: String,
    pub head_blob: Option<String>,
    pub last_seen: Option<String>,
    pub last_written: Option<String>,
    pub updated_at: i64,
}

pub fn lookup_entry(conn: &Connection, path: &str, session_id: &str) -> rusqlite::Result<Option<CasEntry>> {
    let mut stmt = conn.prepare(
        "SELECT path, session_id, head_blob, last_seen, last_written, updated_at
         FROM cas_journal WHERE path = ?1 AND session_id = ?2"
    )?;
    let mut rows = stmt.query(params![path, session_id])?;
    if let Some(row) = rows.next()? {
        Ok(Some(CasEntry {
            path: row.get(0)?,
            session_id: row.get(1)?,
            head_blob: row.get(2)?,
            last_seen: row.get(3)?,
            last_written: row.get(4)?,
            updated_at: row.get(5)?,
        }))
    } else {
        Ok(None)
    }
}

pub fn insert_entry(
    conn: &Connection,
    path: &str,
    session_id: &str,
    head: Option<&str>,
    seen: Option<&str>,
    written: Option<&str>,
) -> rusqlite::Result<usize> {
    let now = epoch_now();
    conn.execute(
        "INSERT INTO cas_journal (path, session_id, head_blob, last_seen, last_written, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![path, session_id, head, seen, written, now],
    )
}

pub fn update_last_seen(
    conn: &Connection,
    path: &str,
    session_id: &str,
    blob: &str,
) -> rusqlite::Result<usize> {
    let now = epoch_now();
    conn.execute(
        "UPDATE cas_journal SET last_seen = ?1, updated_at = ?2
         WHERE path = ?3 AND session_id = ?4",
        params![blob, now, path, session_id],
    )
}

pub fn upsert_seen_and_written(
    conn: &Connection,
    path: &str,
    session_id: &str,
    head: Option<&str>,
    blob: &str,
) -> rusqlite::Result<usize> {
    let now = epoch_now();
    conn.execute(
        "INSERT INTO cas_journal (path, session_id, head_blob, last_seen, last_written, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(path, session_id) DO UPDATE SET
             last_seen = ?4,
             last_written = ?5,
             updated_at = ?6",
        params![path, session_id, head, blob, blob, now],
    )
}

pub fn upsert_seen_only(
    conn: &Connection,
    path: &str,
    session_id: &str,
    head: Option<&str>,
    blob: &str,
) -> rusqlite::Result<usize> {
    let now = epoch_now();
    conn.execute(
        "INSERT INTO cas_journal (path, session_id, head_blob, last_seen, last_written, updated_at)
         VALUES (?1, ?2, ?3, ?4, NULL, ?5)
         ON CONFLICT(path, session_id) DO UPDATE SET
             last_seen = ?4,
             updated_at = ?5",
        params![path, session_id, head, blob, now],
    )
}

pub fn probabilistic_gc(conn: &Connection) {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u8)
        .unwrap_or(1);
    if nanos == 0 {
        let _ = conn.execute(
            "DELETE FROM cas_journal WHERE updated_at < strftime('%s', 'now') - 604800",
            [],
        );
    }
}

pub fn epoch_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn tmp_db_path() -> PathBuf {
        let mut p = env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        p.push(format!("cas_test_{}.db", nanos));
        p
    }

    #[test]
    fn test_migrate_insert_lookup_update() {
        let path = tmp_db_path();
        let mut conn = open_db(&path).unwrap();
        migrate(&mut conn).unwrap();

        insert_entry(&conn, "/foo/bar.rs", "sess-1", Some("head1"), Some("seen1"), None).unwrap();

        let entry = lookup_entry(&conn, "/foo/bar.rs", "sess-1").unwrap().unwrap();
        assert_eq!(entry.last_seen.as_deref(), Some("seen1"));
        assert_eq!(entry.last_written, None);

        update_last_seen(&conn, "/foo/bar.rs", "sess-1", "seen2").unwrap();
        let entry2 = lookup_entry(&conn, "/foo/bar.rs", "sess-1").unwrap().unwrap();
        assert_eq!(entry2.last_seen.as_deref(), Some("seen2"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_upsert_seen_and_written_conflict() {
        let path = tmp_db_path();
        let mut conn = open_db(&path).unwrap();
        migrate(&mut conn).unwrap();

        upsert_seen_and_written(&conn, "/a/b.rs", "sess-2", Some("h1"), "blob1").unwrap();
        let e1 = lookup_entry(&conn, "/a/b.rs", "sess-2").unwrap().unwrap();
        assert_eq!(e1.last_seen.as_deref(), Some("blob1"));
        assert_eq!(e1.last_written.as_deref(), Some("blob1"));

        upsert_seen_and_written(&conn, "/a/b.rs", "sess-2", Some("h1"), "blob2").unwrap();
        let e2 = lookup_entry(&conn, "/a/b.rs", "sess-2").unwrap().unwrap();
        assert_eq!(e2.last_seen.as_deref(), Some("blob2"));
        assert_eq!(e2.last_written.as_deref(), Some("blob2"));

        let _ = std::fs::remove_file(&path);
    }
}
