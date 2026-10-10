use crate::{
    Result,
    schedule::{Task, conflict},
};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::path::Path;

pub struct Store {
    pub db: Connection,
}
impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let db = Connection::open(path)
            .map_err(|e| format!("初始化数据库 {} 失败：{e}", path.display()))?;
        db.busy_timeout(std::time::Duration::from_secs(8))?;
        db.execute_batch("PRAGMA journal_mode=WAL;")?;
        // Control requests open a fresh connection. Do not acquire a schema write
        // lock on every status read while another process is applying timers.
        let tables: i64 = db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('tasks','records','occurrences','metadata','pending')",
            [], |r| r.get(0),
        )?;
        if tables != 5 {
            db.execute_batch("
          CREATE TABLE IF NOT EXISTS tasks(id TEXT PRIMARY KEY,body TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS records(id INTEGER PRIMARY KEY,task_id TEXT,name TEXT,at TEXT,status TEXT,message TEXT);
          CREATE TABLE IF NOT EXISTS occurrences(task_id TEXT,at TEXT,PRIMARY KEY(task_id,at));
          CREATE TABLE IF NOT EXISTS metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS pending(singleton INTEGER PRIMARY KEY CHECK(singleton=1),body TEXT NOT NULL);")?;
        }
        Ok(Self { db })
    }
    pub fn state_view(&self, now: DateTime<Utc>, power: Value) -> Result<Value> {
        // Recovery can wait for the next poll when a writer is active. WAL lets
        // the page still read committed tasks and the application status.
        self.db.busy_timeout(std::time::Duration::ZERO)?;
        let recovery = match self.db.execute_batch("BEGIN IMMEDIATE") {
            Ok(()) => {
                let result = self.recover_expired_pending(now);
                if result.is_ok() {
                    self.db.execute_batch("COMMIT").map_err(Into::into)
                } else {
                    let _ = self.db.execute_batch("ROLLBACK");
                    result
                }
            }
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.code == rusqlite::ErrorCode::DatabaseBusy =>
            {
                Ok(())
            }
            Err(e) => Err(e.into()),
        };
        self.db.busy_timeout(std::time::Duration::from_secs(8))?;
        recovery?;
        self.db.execute_batch("BEGIN")?;
        let view = self.view(now, power);
        self.db.execute_batch("ROLLBACK")?;
        view
    }
    pub fn tasks(&self) -> Result<Vec<Task>> {
        let mut query = self.db.prepare("SELECT body FROM tasks ORDER BY id")?;
        let rows = query.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|row| {
            let t: Task = serde_json::from_str(&row?)?;
            t.validate()?;
            Ok(t)
        })
        .collect()
    }
    pub fn meta(&self, key: &str) -> String {
        self.db
            .query_row("SELECT value FROM metadata WHERE key=?", [key], |r| {
                r.get(0)
            })
            .unwrap_or_default()
    }
    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.db.execute(
            "INSERT OR REPLACE INTO metadata VALUES(?,?)",
            params![key, value],
        )?;
        Ok(())
    }
    pub fn pending(&self) -> Result<Option<Value>> {
        use rusqlite::OptionalExtension;
        let text: Option<String> = self
            .db
            .query_row("SELECT body FROM pending WHERE singleton=1", [], |r| {
                r.get(0)
            })
            .optional()?;
        text.map(|s| serde_json::from_str(&s).map_err(Into::into))
            .transpose()
    }
    pub fn record(&self, t: &Task, status: &str, message: &str, now: DateTime<Utc>) -> Result<()> {
        self.db.execute(
            "INSERT INTO records(task_id,name,at,status,message) VALUES(?,?,?,?,?)",
            params![t.id, t.name, now.to_rfc3339(), status, message],
        )?;
        Ok(())
    }
    pub fn save(&self, mut task: Task, now: DateTime<Utc>) -> Result<()> {
        task.validate()?;
        if task.enabled && task.repeat == "once" && task.next(now).is_none() {
            return Err("单次任务时间必须在未来".into());
        }
        if self
            .pending()?
            .as_ref()
            .is_some_and(|p| p["taskId"] == task.id)
        {
            return Err("请先取消正在倒计时的任务".into());
        }
        let tasks = self.tasks()?;
        if !task.id.is_empty() && !tasks.iter().any(|t| t.id == task.id) {
            return Err("任务不存在".into());
        }
        for t in &tasks {
            if t.id != task.id && conflict(&task, t, now) {
                return Err(format!("与任务“{}”的时间冲突", t.name).into());
            }
        }
        if task.id.is_empty() {
            // Database lock serializes generation, and collision is explicitly checked.
            task.id = format!("r{:x}", now.timestamp_nanos_opt().ok_or("无效时间")?);
            if tasks.iter().any(|t| t.id == task.id) {
                return Err("任务 ID 冲突，请重试".into());
            }
        }
        self.db.execute(
            "INSERT OR REPLACE INTO tasks VALUES(?,?)",
            params![task.id, serde_json::to_string(&task)?],
        )?;
        Ok(())
    }
    pub fn delete(&self, id: &str) -> Result<()> {
        if self.pending()?.as_ref().is_some_and(|p| p["taskId"] == id) {
            return Err("请先取消正在倒计时的任务".into());
        }
        if self.db.execute("DELETE FROM tasks WHERE id=?", [id])? == 0 {
            return Err("任务不存在".into());
        }
        Ok(())
    }
    pub fn cancel(&self, now: DateTime<Utc>) -> Result<()> {
        let p = self.pending()?.ok_or("没有待取消的关机")?;
        let t: Task = serde_json::from_value(p["task"].clone())?;
        self.record(&t, "cancelled", "用户取消本次关机", now)?;
        self.db.execute("DELETE FROM pending", [])?;
        Ok(())
    }
    pub fn begin_countdown(
        &self,
        id: &str,
        now: DateTime<Utc>,
        protected: bool,
    ) -> Result<Option<String>> {
        self.recover_expired_pending(now)?;
        let Some(t) = self
            .tasks()?
            .into_iter()
            .find(|t| t.id == id && t.action == "shutdown")
        else {
            return Ok(None);
        };
        let Some(key) = t.occurrence(now) else {
            return Ok(None);
        };
        if self.db.execute(
            "INSERT OR IGNORE INTO occurrences VALUES(?,?)",
            params![id, key],
        )? == 0
        {
            return Ok(None);
        }
        if protected {
            self.record(&t, "skipped", "启动保护期，跳过本次关机", now)?;
            return Ok(None);
        }
        if self.pending()?.is_some() {
            self.record(&t, "skipped", "已有任务正在倒计时", now)?;
            return Ok(None);
        }
        let token = format!("{}-{}", id, now.timestamp());
        let p = json!({"taskId":id,"name":t.name,"task":t,"deadline":(now+Duration::seconds(60)).to_rfc3339(),"token":token});
        self.record(&t, "countdown", "进入 60 秒关机倒计时", now)?;
        self.db
            .execute("INSERT INTO pending VALUES(1,?)", [p.to_string()])?;
        Ok(Some(token))
    }
    pub fn clear_stale_pending(&self, now: DateTime<Utc>) -> Result<()> {
        if let Some(p) = self.pending()? {
            let t: Task = serde_json::from_value(p["task"].clone())?;
            self.record(&t, "skipped", "执行进程中断，取消本次关机", now)?;
            self.db.execute("DELETE FROM pending", [])?;
        }
        Ok(())
    }
    pub fn recover_expired_pending(&self, now: DateTime<Utc>) -> Result<()> {
        if let Some(p) = self.pending()? {
            let deadline = p["deadline"]
                .as_str()
                .ok_or("倒计时状态损坏")?
                .parse::<DateTime<Utc>>()?;
            if now > deadline + Duration::seconds(60) {
                self.clear_stale_pending(now)?;
            }
        }
        Ok(())
    }
    pub fn view(&self, now: DateTime<Utc>, power: Value) -> Result<Value> {
        let tasks = self.tasks()?;
        let next: serde_json::Map<String, Value> = tasks
            .iter()
            .map(|t| (t.id.clone(), json!(t.next(now).map(|n| n.to_rfc3339()))))
            .collect();
        let mut query = self.db.prepare(
            "SELECT task_id,name,at,status,message FROM records ORDER BY id DESC LIMIT 100",
        )?;
        let records=query.query_map([],|r|Ok(json!({"taskId":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"at":r.get::<_,String>(2)?,"status":r.get::<_,String>(3)?,"message":r.get::<_,String>(4)?})))?.collect::<std::result::Result<Vec<_>,_>>()?;
        let error = [self.meta("apply_error"), self.meta("rtc_error")]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("；");
        Ok(
            json!({"tasks":tasks,"next":next,"records":records,"pending":self.pending()?,"power":power,"now":now.to_rfc3339(),"protectionUntil":self.meta("protection_until"),"error":error}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_stays_readable_while_plans_are_applying() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scheduler.db");
        let writer = Store::open(&path).unwrap();
        let (_, mut t, now) = setup();
        t.id.clear();
        writer.save(t, now).unwrap();
        writer.set_meta("apply_error", "applying").unwrap();
        writer.db.execute_batch("BEGIN IMMEDIATE").unwrap();
        writer.set_meta("apply_error", "uncommitted").unwrap();
        let started = std::time::Instant::now();
        let reader = Store::open(&path).unwrap();
        let view = reader.state_view(now, json!({})).unwrap();
        assert_eq!(view["tasks"].as_array().unwrap().len(), 1);
        assert_eq!(view["error"], "applying");
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        writer.db.execute_batch("ROLLBACK").unwrap();
        // Polling still recovers interrupted countdowns once the writer is free.
        let id = writer.tasks().unwrap()[0].id.clone();
        writer.begin_countdown(&id, now, false).unwrap();
        let view = reader
            .state_view(now + Duration::seconds(121), json!({}))
            .unwrap();
        assert!(view["pending"].is_null());
        assert_eq!(view["records"][0]["status"], "skipped");
    }
    fn setup() -> (Store, Task, DateTime<Utc>) {
        let s = Store::open(Path::new(":memory:")).unwrap();
        let now = "2026-10-10T15:00:00Z".parse().unwrap();
        let t:Task=serde_json::from_value(json!({"name":"off","action":"shutdown","repeat":"daily","time":"23:00","timezone":"Asia/Shanghai","enabled":true})).unwrap();
        s.save(t, now).unwrap();
        let t = s.tasks().unwrap().remove(0);
        (s, t, now)
    }
    #[test]
    fn cancel_and_dedup() {
        let (s, t, now) = setup();
        assert!(s.begin_countdown(&t.id, now, false).unwrap().is_some());
        assert!(s.delete(&t.id).is_err());
        s.cancel(now).unwrap();
        assert!(s.pending().unwrap().is_none());
        assert!(s.begin_countdown(&t.id, now, false).unwrap().is_none());
    }
    #[test]
    fn protection_and_missed() {
        let (s, t, now) = setup();
        assert!(s.begin_countdown(&t.id, now, true).unwrap().is_none());
        assert!(
            s.begin_countdown(&t.id, now + Duration::minutes(2), false)
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn legacy_schema_is_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scheduler.db");
        let old = Connection::open(&path).unwrap();
        old.execute_batch("CREATE TABLE tasks(id TEXT PRIMARY KEY,body TEXT NOT NULL); CREATE TABLE records(id INTEGER PRIMARY KEY,task_id TEXT,name TEXT,at TEXT,status TEXT,message TEXT); CREATE TABLE occurrences(task_id TEXT,at TEXT,PRIMARY KEY(task_id,at));").unwrap();
        let (_, mut t, now) = setup();
        t.id = "legacy123".into();
        old.execute(
            "INSERT INTO tasks VALUES(?,?)",
            params![t.id, serde_json::to_string(&t).unwrap()],
        )
        .unwrap();
        drop(old);
        let s = Store::open(&path).unwrap();
        assert_eq!(s.tasks().unwrap()[0].id, "legacy123");
        assert!(s.view(now, json!({})).is_ok());
    }
    #[test]
    fn recover_crashed_countdown_and_allow_next_day() {
        let (s, t, now) = setup();
        s.begin_countdown(&t.id, now, false).unwrap();
        s.recover_expired_pending(now + Duration::seconds(121))
            .unwrap();
        assert!(s.pending().unwrap().is_none());
        assert!(
            s.begin_countdown(&t.id, now + Duration::days(1), false)
                .unwrap()
                .is_some()
        );
    }
    #[test]
    fn cancellation_is_visible_to_another_connection() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scheduler.db");
        let (source, t, now) = setup();
        let executor = Store::open(&path).unwrap();
        executor
            .save(
                Task {
                    id: String::new(),
                    ..t
                },
                now,
            )
            .unwrap();
        let id = executor.tasks().unwrap()[0].id.clone();
        executor.begin_countdown(&id, now, false).unwrap();
        let controller = Store::open(&path).unwrap();
        controller.cancel(now).unwrap();
        assert!(executor.pending().unwrap().is_none());
        assert!(executor.begin_countdown(&id, now, false).unwrap().is_none());
        drop(source);
    }
}
