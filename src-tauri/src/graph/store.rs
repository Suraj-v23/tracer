use rusqlite::{Connection, Result as SqlResult, params};
use serde::{Deserialize, Serialize};
use std::path::Path;

// ─── Data types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GraphNode {
    pub id:            i64,
    pub path:          String,
    pub name:          String,
    pub kind:          String,   // "file" | "directory"
    pub size:          u64,
    pub extension:     Option<String>,
    pub modified_secs: Option<i64>,
    pub created_secs:  Option<i64>,
    pub content_hash:  Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub path:          String,
    pub name:          String,
    pub kind:          String,
    pub size:          u64,
    pub size_human:    String,
    pub extension:     Option<String>,
    pub modified_secs: Option<i64>,
    pub snippet:       Option<String>,
    pub score:         f64,
}

// ─── Store ───────────────────────────────────────────────────────────────────

pub struct Store {
    pub conn: Connection,
}

impl Store {
    pub fn open(db_path: &Path) -> SqlResult<Self> {
        if let Some(parent) = db_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                eprintln!("[graph/store] could not create db dir: {e}");
            }
        }
        let conn = Connection::open(db_path)?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    pub fn open_in_memory() -> SqlResult<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> SqlResult<()> {
        // journal_mode=WAL returns a result row, so it must be consumed via query_row
        self.conn.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(()))?;
        self.conn.execute_batch(r#"
            PRAGMA synchronous=NORMAL;
            PRAGMA foreign_keys=ON;
            CREATE TABLE IF NOT EXISTS nodes (
                id            INTEGER PRIMARY KEY,
                path          TEXT UNIQUE NOT NULL,
                name          TEXT NOT NULL,
                kind          TEXT NOT NULL,
                size          INTEGER DEFAULT 0,
                extension     TEXT,
                modified_secs INTEGER,
                created_secs  INTEGER,
                content_hash  TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_nodes_size     ON nodes(size);
            CREATE INDEX IF NOT EXISTS idx_nodes_ext      ON nodes(extension);
            CREATE INDEX IF NOT EXISTS idx_nodes_modified ON nodes(modified_secs);
            CREATE INDEX IF NOT EXISTS idx_nodes_name     ON nodes(name COLLATE NOCASE);

            CREATE TABLE IF NOT EXISTS edges (
                from_id INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
                to_id   INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
                kind    TEXT NOT NULL,
                UNIQUE(from_id, to_id, kind)
            );
            CREATE INDEX IF NOT EXISTS idx_edges_from ON edges(from_id);
            CREATE INDEX IF NOT EXISTS idx_edges_to   ON edges(to_id);

            CREATE TABLE IF NOT EXISTS indexed_folders (
                path       TEXT PRIMARY KEY,
                added_secs INTEGER
            );

            CREATE TABLE IF NOT EXISTS settings (
                key   TEXT PRIMARY KEY,
                value TEXT
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS fts_content USING fts5(
                content,
                tokenize='porter ascii'
            );

            CREATE TABLE IF NOT EXISTS fts_node_map (
                rowid   INTEGER PRIMARY KEY,
                node_id INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE
            );
        "#)
    }

    // ── Node CRUD ────────────────────────────────────────────────────────────

    pub fn upsert_node(&self, node: &GraphNode) -> SqlResult<i64> {
        self.conn.execute(
            r#"INSERT INTO nodes (path, name, kind, size, extension, modified_secs, created_secs, content_hash)
               VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
               ON CONFLICT(path) DO UPDATE SET
                 name=excluded.name, kind=excluded.kind, size=excluded.size,
                 extension=excluded.extension, modified_secs=excluded.modified_secs,
                 created_secs=excluded.created_secs, content_hash=excluded.content_hash"#,
            params![node.path, node.name, node.kind, node.size as i64,
                    node.extension, node.modified_secs, node.created_secs, node.content_hash],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn delete_node(&self, path: &str) -> SqlResult<()> {
        self.conn.execute("DELETE FROM nodes WHERE path=?1", params![path])?;
        Ok(())
    }

    pub fn get_node_id(&self, path: &str) -> SqlResult<Option<i64>> {
        let mut stmt = self.conn.prepare_cached("SELECT id FROM nodes WHERE path=?1")?;
        let mut rows = stmt.query(params![path])?;
        Ok(rows.next()?.map(|r| r.get(0)).transpose()?)
    }

    pub fn node_count(&self) -> SqlResult<i64> {
        self.conn.query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))
    }

    // ── Edge CRUD ────────────────────────────────────────────────────────────

    pub fn upsert_edge(&self, from_path: &str, to_path: &str, kind: &str) -> SqlResult<()> {
        let from_id = match self.get_node_id(from_path)? { Some(id) => id, None => return Ok(()) };
        let to_id   = match self.get_node_id(to_path)?   { Some(id) => id, None => return Ok(()) };
        self.conn.execute(
            "INSERT OR IGNORE INTO edges (from_id, to_id, kind) VALUES (?1,?2,?3)",
            params![from_id, to_id, kind],
        )?;
        Ok(())
    }

    // ── Queries ──────────────────────────────────────────────────────────────

    pub fn query_metadata(
        &self,
        name_contains:  Option<&str>,
        extension:      Option<&str>,
        kind:           Option<&str>,
        size_gt:        Option<u64>,
        size_lt:        Option<u64>,
        modified_after: Option<i64>,
    ) -> SqlResult<Vec<SearchResult>> {
        let mut sql = String::from(
            "SELECT path, name, kind, size, extension, modified_secs FROM nodes WHERE 1=1"
        );
        let mut param_values: Vec<Box<dyn rusqlite::ToSql>> = vec![];

        if let Some(n) = name_contains {
            param_values.push(Box::new(format!("%{n}%")));
            sql.push_str(&format!(" AND name LIKE ?{}", param_values.len()));
        }
        if let Some(e) = extension {
            param_values.push(Box::new(e.to_string()));
            sql.push_str(&format!(" AND extension=?{}", param_values.len()));
        }
        if let Some(k) = kind {
            param_values.push(Box::new(k.to_string()));
            sql.push_str(&format!(" AND kind=?{}", param_values.len()));
        }
        if let Some(sg) = size_gt {
            param_values.push(Box::new(sg as i64));
            sql.push_str(&format!(" AND size>?{}", param_values.len()));
        }
        if let Some(sl) = size_lt {
            param_values.push(Box::new(sl as i64));
            sql.push_str(&format!(" AND size<?{}", param_values.len()));
        }
        if let Some(ma) = modified_after {
            param_values.push(Box::new(ma));
            sql.push_str(&format!(" AND modified_secs>?{}", param_values.len()));
        }
        sql.push_str(" ORDER BY size DESC LIMIT 200");

        let params_ref: Vec<&dyn rusqlite::ToSql> = param_values.iter().map(|b| b.as_ref()).collect();
        let mut stmt = self.conn.prepare(&sql)?;
        let results = stmt.query_map(params_ref.as_slice(), row_to_result)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(results)
    }

    /// Find duplicates of a specific file (by path). If path is "/" returns
    /// all duplicate files across the entire index.
    pub fn find_duplicates(&self, path: &str) -> SqlResult<Vec<SearchResult>> {
        if path == "/" {
            return self.find_all_duplicates();
        }
        let mut stmt = self.conn.prepare(r#"
            SELECT n2.path, n2.name, n2.kind, n2.size, n2.extension, n2.modified_secs
            FROM nodes n1
            JOIN edges e ON e.from_id = n1.id AND e.kind = 'duplicate'
            JOIN nodes n2 ON n2.id = e.to_id
            WHERE n1.path = ?1
        "#)?;
        let results = stmt.query_map(params![path], row_to_result)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(results)
    }

    /// Return all files that share a content_hash with at least one other file.
    pub fn find_all_duplicates(&self) -> SqlResult<Vec<SearchResult>> {
        let mut stmt = self.conn.prepare(r#"
            SELECT path, name, kind, size, extension, modified_secs
            FROM nodes
            WHERE content_hash IS NOT NULL
              AND content_hash IN (
                  SELECT content_hash FROM nodes
                  WHERE content_hash IS NOT NULL
                  GROUP BY content_hash
                  HAVING COUNT(*) > 1
              )
            ORDER BY content_hash, size DESC
            LIMIT 200
        "#)?;
        let results = stmt.query_map([], row_to_result)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(results)
    }

    pub fn get_children(&self, path: &str, depth: usize) -> SqlResult<Vec<SearchResult>> {
        let pattern = if path.ends_with('/') {
            format!("{path}%")
        } else {
            format!("{path}/%")
        };
        let max_slashes = path.matches('/').count() + depth;
        let mut stmt = self.conn.prepare(r#"
            SELECT path, name, kind, size, extension, modified_secs
            FROM nodes
            WHERE path LIKE ?1
              AND (LENGTH(path) - LENGTH(REPLACE(path, '/', ''))) <= ?2
            ORDER BY size DESC LIMIT 200
        "#)?;
        let results = stmt.query_map(params![pattern, max_slashes as i64], row_to_result)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(results)
    }

    pub fn get_setting(&self, key: &str) -> SqlResult<Option<String>> {
        let mut stmt = self.conn.prepare_cached("SELECT value FROM settings WHERE key=?1")?;
        let mut rows = stmt.query(params![key])?;
        Ok(rows.next()?.map(|r| r.get(0)).transpose()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> SqlResult<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO settings (key,value) VALUES (?1,?2)",
            params![key, value],
        )?;
        Ok(())
    }

    // ── FTS Content Indexing ──────────────────────────────────────────────────

    pub fn index_content(&self, node_id: i64, text: &str) -> SqlResult<()> {
        // Remove old entry if exists
        let old_rowid: Option<i64> = {
            let mut s = self.conn.prepare_cached(
                "SELECT rowid FROM fts_node_map WHERE node_id=?1"
            )?;
            let mut rows = s.query(params![node_id])?;
            rows.next()?.map(|r| r.get(0)).transpose()?
        };
        if let Some(rowid) = old_rowid {
            self.conn.execute("DELETE FROM fts_content WHERE rowid=?1", params![rowid])?;
            self.conn.execute("DELETE FROM fts_node_map WHERE node_id=?1", params![node_id])?;
        }

        self.conn.execute(
            "INSERT INTO fts_content(content) VALUES (?1)",
            params![text],
        )?;
        let rowid = self.conn.last_insert_rowid();
        self.conn.execute(
            "INSERT INTO fts_node_map(rowid, node_id) VALUES (?1, ?2)",
            params![rowid, node_id],
        )?;
        Ok(())
    }

    pub fn content_search(&self, query: &str) -> SqlResult<Vec<SearchResult>> {
        let mut stmt = self.conn.prepare(r#"
            SELECT n.path, n.name, n.kind, n.size, n.extension, n.modified_secs,
                   snippet(fts_content, 0, '[', ']', '...', 16) AS snip
            FROM fts_content f
            JOIN fts_node_map m ON m.rowid = f.rowid
            JOIN nodes n ON n.id = m.node_id
            WHERE fts_content MATCH ?1
            ORDER BY rank
            LIMIT 100
        "#)?;
        let results = stmt.query_map(params![query], |row| {
            let size: i64 = row.get(3)?;
            Ok(SearchResult {
                path:          row.get(0)?,
                name:          row.get(1)?,
                kind:          row.get(2)?,
                size:          size as u64,
                size_human:    format_size(size as u64),
                extension:     row.get(4)?,
                modified_secs: row.get(5)?,
                snippet:       row.get(6)?,
                score:         1.0,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();
        Ok(results)
    }

    // ── Indexed Folders ───────────────────────────────────────────────────────

    pub fn add_indexed_folder(&self, path: &str) -> SqlResult<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        self.conn.execute(
            "INSERT OR REPLACE INTO indexed_folders(path, added_secs) VALUES (?1, ?2)",
            params![path, now],
        )?;
        Ok(())
    }

    pub fn remove_indexed_folder(&self, path: &str) -> SqlResult<()> {
        self.conn.execute(
            "DELETE FROM indexed_folders WHERE path=?1",
            params![path],
        )?;
        Ok(())
    }

    pub fn list_indexed_folders(&self) -> SqlResult<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT path FROM indexed_folders ORDER BY added_secs")?;
        let results = stmt.query_map([], |r| r.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(results)
    }

    pub fn is_folder_indexed(&self, path: &str) -> SqlResult<bool> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT 1 FROM indexed_folders WHERE ?1 LIKE path || '%' LIMIT 1"
        )?;
        let mut rows = stmt.query(params![path])?;
        Ok(rows.next()?.is_some())
    }

    // ── Import Graph Queries ──────────────────────────────────────────────────

    pub fn get_imports(&self, path: &str) -> SqlResult<Vec<SearchResult>> {
        let mut stmt = self.conn.prepare(r#"
            SELECT n2.path, n2.name, n2.kind, n2.size, n2.extension, n2.modified_secs
            FROM nodes n1
            JOIN edges e  ON e.from_id = n1.id AND e.kind = 'imports'
            JOIN nodes n2 ON n2.id = e.to_id
            WHERE n1.path = ?1
            ORDER BY n2.name
            LIMIT 200
        "#)?;
        let results = stmt.query_map(params![path], row_to_result)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(results)
    }

    pub fn get_importers(&self, path: &str) -> SqlResult<Vec<SearchResult>> {
        let mut stmt = self.conn.prepare(r#"
            SELECT n2.path, n2.name, n2.kind, n2.size, n2.extension, n2.modified_secs
            FROM nodes n1
            JOIN edges e  ON e.to_id = n1.id AND e.kind = 'imports'
            JOIN nodes n2 ON n2.id = e.from_id
            WHERE n1.path = ?1
            ORDER BY n2.name
            LIMIT 200
        "#)?;
        let results = stmt.query_map(params![path], row_to_result)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(results)
    }

    pub fn import_count(&self, path: &str) -> SqlResult<i64> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM nodes n JOIN edges e ON e.from_id=n.id AND e.kind='imports' WHERE n.path=?1",
            params![path],
            |r| r.get(0),
        )
    }

    pub fn importer_count(&self, path: &str) -> SqlResult<i64> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM nodes n JOIN edges e ON e.to_id=n.id AND e.kind='imports' WHERE n.path=?1",
            params![path],
            |r| r.get(0),
        )
    }
}

fn row_to_result(row: &rusqlite::Row<'_>) -> rusqlite::Result<SearchResult> {
    let size: i64 = row.get(3)?;
    Ok(SearchResult {
        path:          row.get(0)?,
        name:          row.get(1)?,
        kind:          row.get(2)?,
        size:          size as u64,
        size_human:    format_size(size as u64),
        extension:     row.get(4)?,
        modified_secs: row.get(5)?,
        snippet:       None,
        score:         1.0,
    })
}

pub fn format_size(bytes: u64) -> String {
    if bytes == 0 { return "0 B".into(); }
    let units = ["B","KB","MB","GB","TB"];
    let mut size = bytes as f64;
    let mut idx = 0;
    while size >= 1000.0 && idx < units.len() - 1 { size /= 1000.0; idx += 1; }
    if idx == 0 { format!("{bytes} B") } else { format!("{:.1} {}", size, units[idx]) }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(path: &str, name: &str, kind: &str, size: u64) -> GraphNode {
        GraphNode { id: 0, path: path.into(), name: name.into(), kind: kind.into(),
                    size, extension: None, modified_secs: None, created_secs: None, content_hash: None }
    }

    #[test]
    fn upsert_and_count() {
        let store = Store::open_in_memory().unwrap();
        store.upsert_node(&make_node("/a/b.txt", "b.txt", "file", 1024)).unwrap();
        assert_eq!(store.node_count().unwrap(), 1);
    }

    #[test]
    fn upsert_updates_existing() {
        let store = Store::open_in_memory().unwrap();
        store.upsert_node(&make_node("/a/b.txt", "b.txt", "file", 1024)).unwrap();
        store.upsert_node(&make_node("/a/b.txt", "b.txt", "file", 2048)).unwrap();
        assert_eq!(store.node_count().unwrap(), 1);
    }

    #[test]
    fn delete_node_removes_it() {
        let store = Store::open_in_memory().unwrap();
        store.upsert_node(&make_node("/a/b.txt", "b.txt", "file", 100)).unwrap();
        store.delete_node("/a/b.txt").unwrap();
        assert_eq!(store.node_count().unwrap(), 0);
    }

    #[test]
    fn query_by_name_contains() {
        let store = Store::open_in_memory().unwrap();
        store.upsert_node(&make_node("/a/readme.md", "readme.md", "file", 500)).unwrap();
        store.upsert_node(&make_node("/a/main.rs",   "main.rs",   "file", 200)).unwrap();
        let results = store.query_metadata(Some("readme"), None, None, None, None, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "readme.md");
    }

    #[test]
    fn query_by_size_gt() {
        let store = Store::open_in_memory().unwrap();
        store.upsert_node(&make_node("/a/big.mp4",   "big.mp4",   "file", 500_000_000)).unwrap();
        store.upsert_node(&make_node("/a/small.txt", "small.txt", "file", 1_000)).unwrap();
        let results = store.query_metadata(None, None, None, Some(100_000_000), None, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "big.mp4");
    }

    #[test]
    fn find_duplicates_returns_peers() {
        let store = Store::open_in_memory().unwrap();
        let mut n1 = make_node("/a/f1.txt", "f1.txt", "file", 100);
        n1.content_hash = Some("abc123".into());
        let mut n2 = make_node("/b/f2.txt", "f2.txt", "file", 100);
        n2.content_hash = Some("abc123".into());
        store.upsert_node(&n1).unwrap();
        store.upsert_node(&n2).unwrap();
        store.upsert_edge("/a/f1.txt", "/b/f2.txt", "duplicate").unwrap();
        let dupes = store.find_duplicates("/a/f1.txt").unwrap();
        assert_eq!(dupes.len(), 1);
        assert_eq!(dupes[0].name, "f2.txt");
    }

    #[test]
    fn format_size_correct() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(1_500), "1.5 KB");
        assert_eq!(format_size(1_500_000), "1.5 MB");
    }

    #[test]
    fn fts_index_and_search() {
        let store = Store::open_in_memory().unwrap();
        store.upsert_node(&make_node("/a/readme.md", "readme.md", "file", 100)).unwrap();
        let id = store.get_node_id("/a/readme.md").unwrap().unwrap();
        store.index_content(id, "# Hello World\nThis file talks about authentication tokens.").unwrap();
        let results = store.content_search("authentication").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "readme.md");
        assert!(results[0].snippet.is_some());
    }

    #[test]
    fn fts_returns_empty_for_no_match() {
        let store = Store::open_in_memory().unwrap();
        store.upsert_node(&make_node("/a/readme.md", "readme.md", "file", 100)).unwrap();
        let id = store.get_node_id("/a/readme.md").unwrap().unwrap();
        store.index_content(id, "hello world").unwrap();
        let results = store.content_search("zxqwerty").unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn indexed_folders_crud() {
        let store = Store::open_in_memory().unwrap();
        store.add_indexed_folder("/home/projects").unwrap();
        let folders = store.list_indexed_folders().unwrap();
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0], "/home/projects");
        store.remove_indexed_folder("/home/projects").unwrap();
        let folders = store.list_indexed_folders().unwrap();
        assert_eq!(folders.len(), 0);
    }

    #[test]
    fn get_imports_returns_direct_deps() {
        let store = Store::open_in_memory().unwrap();
        store.upsert_node(&make_node("/a/main.ts",  "main.ts",  "file", 100)).unwrap();
        store.upsert_node(&make_node("/a/utils.ts", "utils.ts", "file", 50)).unwrap();
        store.upsert_node(&make_node("/a/types.ts", "types.ts", "file", 30)).unwrap();
        store.upsert_edge("/a/main.ts", "/a/utils.ts", "imports").unwrap();
        store.upsert_edge("/a/main.ts", "/a/types.ts", "imports").unwrap();
        let imports = store.get_imports("/a/main.ts").unwrap();
        assert_eq!(imports.len(), 2);
        let names: Vec<_> = imports.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"utils.ts"));
        assert!(names.contains(&"types.ts"));
    }

    #[test]
    fn get_importers_returns_reverse_deps() {
        let store = Store::open_in_memory().unwrap();
        store.upsert_node(&make_node("/a/main.ts",  "main.ts",  "file", 100)).unwrap();
        store.upsert_node(&make_node("/a/lib.ts",   "lib.ts",   "file", 100)).unwrap();
        store.upsert_node(&make_node("/a/utils.ts", "utils.ts", "file",  50)).unwrap();
        store.upsert_edge("/a/main.ts", "/a/utils.ts", "imports").unwrap();
        store.upsert_edge("/a/lib.ts",  "/a/utils.ts", "imports").unwrap();
        let importers = store.get_importers("/a/utils.ts").unwrap();
        assert_eq!(importers.len(), 2);
        let names: Vec<_> = importers.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"main.ts"));
        assert!(names.contains(&"lib.ts"));
    }

    #[test]
    fn import_count_on_node() {
        let store = Store::open_in_memory().unwrap();
        store.upsert_node(&make_node("/a/main.ts",  "main.ts",  "file", 100)).unwrap();
        store.upsert_node(&make_node("/a/utils.ts", "utils.ts", "file",  50)).unwrap();
        store.upsert_edge("/a/main.ts", "/a/utils.ts", "imports").unwrap();
        assert_eq!(store.import_count("/a/main.ts").unwrap(),    1);
        assert_eq!(store.importer_count("/a/utils.ts").unwrap(), 1);
    }
}
