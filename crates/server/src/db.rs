//! SQLite logbook: sessions, hands (full record JSON), graded decisions, Hunter exploits.

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use std::path::Path;
use std::sync::Mutex;

pub struct Db {
    conn: Mutex<Connection>,
}

const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
CREATE TABLE IF NOT EXISTS sessions (
    id INTEGER PRIMARY KEY,
    started_at INTEGER NOT NULL,
    config TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'play'
);
CREATE TABLE IF NOT EXISTS hands (
    id INTEGER PRIMARY KEY,
    session_id INTEGER NOT NULL,
    hand_no INTEGER NOT NULL,
    played_at INTEGER NOT NULL,
    hero_seat INTEGER,
    hero_position TEXT,
    hero_cards TEXT,
    board TEXT,
    hero_net INTEGER DEFAULT 0,
    hero_net_adj REAL DEFAULT 0,
    pot INTEGER DEFAULT 0,
    showdown INTEGER DEFAULT 0,
    complete INTEGER DEFAULT 0,
    ev_loss REAL DEFAULT 0,
    record TEXT,
    replay TEXT,
    notes TEXT NOT NULL DEFAULT '',
    tags TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS hands_session ON hands(session_id);
CREATE TABLE IF NOT EXISTS decisions (
    id INTEGER PRIMARY KEY,
    hand_id INTEGER NOT NULL,
    idx INTEGER NOT NULL,
    street TEXT,
    spot_key TEXT,
    kind TEXT,
    grade TEXT,
    ev_loss_bb REAL,
    data TEXT NOT NULL,
    UNIQUE(hand_id, idx)
);
CREATE INDEX IF NOT EXISTS decisions_spot ON decisions(spot_key);
CREATE TABLE IF NOT EXISTS exploits (
    id INTEGER PRIMARY KEY,
    session_id INTEGER NOT NULL,
    hand_no INTEGER,
    spot TEXT,
    observed REAL,
    solver REAL,
    samples INTEGER,
    note TEXT
);
"#;

pub fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64
}

impl Db {
    pub fn open(path: &Path) -> anyhow::Result<Db> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Db { conn: Mutex::new(conn) })
    }

    pub fn new_session(&self, config: &Value, kind: &str) -> i64 {
        let c = self.conn.lock().unwrap();
        c.execute("INSERT INTO sessions (started_at, config, kind) VALUES (?1, ?2, ?3)", params![now(), config.to_string(), kind]).unwrap();
        c.last_insert_rowid()
    }

    pub fn next_hand_no(&self) -> i64 {
        let c = self.conn.lock().unwrap();
        c.query_row("SELECT COALESCE(MAX(hand_no), 0) + 1 FROM hands", [], |r| r.get(0)).unwrap_or(1)
    }

    pub fn new_hand(&self, session_id: i64, hand_no: i64, hero_seat: i64, position: &str, cards: &str) -> i64 {
        let c = self.conn.lock().unwrap();
        c.execute(
            "INSERT INTO hands (session_id, hand_no, played_at, hero_seat, hero_position, hero_cards) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![session_id, hand_no, now(), hero_seat, position, cards],
        )
        .unwrap();
        c.last_insert_rowid()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn finish_hand(&self, id: i64, board: &str, net: i64, net_adj: f64, pot: i64, showdown: bool, record: &Value) {
        let c = self.conn.lock().unwrap();
        c.execute(
            "UPDATE hands SET board = ?2, hero_net = ?3, hero_net_adj = ?4, pot = ?5, showdown = ?6, record = ?7, complete = 1 WHERE id = ?1",
            params![id, board, net, net_adj, pot, showdown as i64, record.to_string()],
        )
        .unwrap();
    }

    pub fn set_replay(&self, id: i64, replay: &Value) {
        let c = self.conn.lock().unwrap();
        c.execute("UPDATE hands SET replay = ?2 WHERE id = ?1", params![id, replay.to_string()]).unwrap();
    }

    pub fn save_decision(&self, hand_id: i64, idx: i64, grade: &Value) {
        let c = self.conn.lock().unwrap();
        let s = |k: &str| grade.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let loss = grade.get("ev_loss_bb").and_then(|v| v.as_f64());
        c.execute(
            "INSERT INTO decisions (hand_id, idx, street, spot_key, kind, grade, ev_loss_bb, data) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(hand_id, idx) DO UPDATE SET street = excluded.street, spot_key = excluded.spot_key, kind = excluded.kind,
             grade = excluded.grade, ev_loss_bb = excluded.ev_loss_bb, data = excluded.data",
            params![hand_id, idx, s("street"), s("spot_key"), s("kind"), s("grade"), loss, grade.to_string()],
        )
        .unwrap();
        c.execute("UPDATE hands SET ev_loss = (SELECT COALESCE(SUM(ev_loss_bb), 0) FROM decisions WHERE hand_id = ?1) WHERE id = ?1", params![hand_id]).unwrap();
    }

    pub fn set_explanation(&self, hand_id: i64, idx: i64, text: &str) {
        let c = self.conn.lock().unwrap();
        let data: Option<String> = c.query_row("SELECT data FROM decisions WHERE hand_id = ?1 AND idx = ?2", params![hand_id, idx], |r| r.get(0)).optional().unwrap();
        if let Some(d) = data {
            if let Ok(mut v) = serde_json::from_str::<Value>(&d) {
                v["explanation"] = Value::String(text.to_string());
                c.execute("UPDATE decisions SET data = ?3 WHERE hand_id = ?1 AND idx = ?2", params![hand_id, idx, v.to_string()]).unwrap();
            }
        }
    }

    pub fn decisions(&self, hand_id: i64) -> Vec<Value> {
        let c = self.conn.lock().unwrap();
        let mut st = c.prepare("SELECT data FROM decisions WHERE hand_id = ?1 ORDER BY idx").unwrap();
        st.query_map(params![hand_id], |r| r.get::<_, String>(0)).unwrap().filter_map(|x| x.ok()).filter_map(|s| serde_json::from_str(&s).ok()).collect()
    }

    pub fn decision(&self, hand_id: i64, idx: i64) -> Option<Value> {
        let c = self.conn.lock().unwrap();
        c.query_row("SELECT data FROM decisions WHERE hand_id = ?1 AND idx = ?2", params![hand_id, idx], |r| r.get::<_, String>(0))
            .optional()
            .unwrap()
            .and_then(|s| serde_json::from_str(&s).ok())
    }

    pub fn hand(&self, id: i64) -> Option<Value> {
        let c = self.conn.lock().unwrap();
        c.query_row(
            "SELECT id, session_id, hand_no, played_at, hero_position, hero_cards, board, hero_net, hero_net_adj, pot, showdown, ev_loss, record, replay, notes, tags FROM hands WHERE id = ?1",
            params![id],
            |r| {
                Ok(serde_json::json!({
                    "id": r.get::<_, i64>(0)?, "session_id": r.get::<_, i64>(1)?, "hand_no": r.get::<_, i64>(2)?,
                    "played_at": r.get::<_, i64>(3)?, "hero_position": r.get::<_, Option<String>>(4)?,
                    "hero_cards": r.get::<_, Option<String>>(5)?, "board": r.get::<_, Option<String>>(6)?,
                    "hero_net": r.get::<_, i64>(7)?, "hero_net_adj": r.get::<_, f64>(8)?, "pot": r.get::<_, i64>(9)?,
                    "showdown": r.get::<_, i64>(10)? == 1, "ev_loss": r.get::<_, f64>(11)?,
                    "record": r.get::<_, Option<String>>(12)?.and_then(|s| serde_json::from_str::<Value>(&s).ok()),
                    "replay": r.get::<_, Option<String>>(13)?.and_then(|s| serde_json::from_str::<Value>(&s).ok()),
                    "notes": r.get::<_, String>(14)?, "tags": r.get::<_, String>(15)?,
                }))
            },
        )
        .optional()
        .unwrap()
    }

    pub fn list_hands(&self, limit: i64, offset: i64, session: Option<i64>, tag: Option<&str>, min_loss: Option<f64>) -> Vec<Value> {
        let c = self.conn.lock().unwrap();
        let mut sql = "SELECT id, session_id, hand_no, played_at, hero_position, hero_cards, board, hero_net, ev_loss, showdown, notes, tags FROM hands WHERE complete = 1".to_string();
        if let Some(s) = session {
            sql.push_str(&format!(" AND session_id = {s}"));
        }
        if let Some(t) = tag {
            sql.push_str(&format!(" AND (',' || tags || ',') LIKE '%,{}' || ',%'", t.replace('\'', "")));
        }
        if let Some(m) = min_loss {
            sql.push_str(&format!(" AND ev_loss >= {m}"));
        }
        sql.push_str(&format!(" ORDER BY id DESC LIMIT {limit} OFFSET {offset}"));
        let mut st = c.prepare(&sql).unwrap();
        st.query_map([], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, i64>(0)?, "session_id": r.get::<_, i64>(1)?, "hand_no": r.get::<_, i64>(2)?,
                "played_at": r.get::<_, i64>(3)?, "hero_position": r.get::<_, Option<String>>(4)?,
                "hero_cards": r.get::<_, Option<String>>(5)?, "board": r.get::<_, Option<String>>(6)?,
                "hero_net": r.get::<_, i64>(7)?, "ev_loss": r.get::<_, f64>(8)?, "showdown": r.get::<_, i64>(9)? == 1,
                "notes": r.get::<_, String>(10)?, "tags": r.get::<_, String>(11)?,
            }))
        })
        .unwrap()
        .filter_map(|x| x.ok())
        .collect()
    }

    pub fn set_notes(&self, id: i64, notes: &str, tags: &str) {
        let c = self.conn.lock().unwrap();
        c.execute("UPDATE hands SET notes = ?2, tags = ?3 WHERE id = ?1", params![id, notes, tags]).unwrap();
    }

    /// (hand id, record) for completed hands, optionally of one session.
    pub fn records(&self, session: Option<i64>) -> Vec<(i64, Value)> {
        let c = self.conn.lock().unwrap();
        let sql = match session {
            Some(s) => format!("SELECT id, record FROM hands WHERE complete = 1 AND session_id = {s} ORDER BY id"),
            None => "SELECT id, record FROM hands WHERE complete = 1 ORDER BY id".to_string(),
        };
        let mut st = c.prepare(&sql).unwrap();
        st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?)))
            .unwrap()
            .filter_map(|x| x.ok())
            .filter_map(|(id, s)| s.and_then(|s| serde_json::from_str(&s).ok()).map(|v| (id, v)))
            .collect()
    }

    pub fn all_decisions(&self, session: Option<i64>) -> Vec<(i64, Value)> {
        let c = self.conn.lock().unwrap();
        let sql = match session {
            Some(s) => format!("SELECT d.hand_id, d.data FROM decisions d JOIN hands h ON h.id = d.hand_id WHERE h.session_id = {s}"),
            None => "SELECT hand_id, data FROM decisions".to_string(),
        };
        let mut st = c.prepare(&sql).unwrap();
        st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            .unwrap()
            .filter_map(|x| x.ok())
            .filter_map(|(id, s)| serde_json::from_str(&s).ok().map(|v| (id, v)))
            .collect()
    }

    pub fn add_exploit(&self, session: i64, e: &strategy::bots::hunter::ExploitRecord) {
        let c = self.conn.lock().unwrap();
        c.execute(
            "INSERT INTO exploits (session_id, hand_no, spot, observed, solver, samples, note) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![session, e.hand_no as i64, e.spot, e.observed as f64, e.solver as f64, e.samples as i64, e.note],
        )
        .unwrap();
    }

    pub fn exploits(&self, session: Option<i64>) -> Vec<Value> {
        let c = self.conn.lock().unwrap();
        let sql = match session {
            Some(s) => format!("SELECT session_id, hand_no, spot, observed, solver, samples, note FROM exploits WHERE session_id = {s} ORDER BY id"),
            None => "SELECT session_id, hand_no, spot, observed, solver, samples, note FROM exploits ORDER BY id".to_string(),
        };
        let mut st = c.prepare(&sql).unwrap();
        st.query_map([], |r| {
            Ok(serde_json::json!({
                "session_id": r.get::<_, i64>(0)?, "hand_no": r.get::<_, i64>(1)?, "spot": r.get::<_, String>(2)?,
                "observed": r.get::<_, f64>(3)?, "solver": r.get::<_, f64>(4)?, "samples": r.get::<_, i64>(5)?, "note": r.get::<_, String>(6)?,
            }))
        })
        .unwrap()
        .filter_map(|x| x.ok())
        .collect()
    }

    pub fn sessions(&self) -> Vec<Value> {
        let c = self.conn.lock().unwrap();
        let mut st = c
            .prepare("SELECT s.id, s.started_at, s.kind, COUNT(h.id), COALESCE(SUM(h.hero_net), 0) FROM sessions s LEFT JOIN hands h ON h.session_id = s.id AND h.complete = 1 GROUP BY s.id ORDER BY s.id DESC")
            .unwrap();
        st.query_map([], |r| {
            Ok(serde_json::json!({"id": r.get::<_, i64>(0)?, "started_at": r.get::<_, i64>(1)?, "kind": r.get::<_, String>(2)?, "hands": r.get::<_, i64>(3)?, "hero_net": r.get::<_, i64>(4)?}))
        })
        .unwrap()
        .filter_map(|x| x.ok())
        .collect()
    }
}
