#[cfg(target_os = "linux")]
mod linux {
    use chrono::{Duration, Utc};
    use fnos_power_scheduler::{
        Result,
        schedule::Task,
        store::Store,
        system::{self, Config, Rtc},
        wire::peer_uid,
    };
    use serde_json::{Value, json};
    use std::{
        fs,
        io::{Read, Write},
        os::{fd::FromRawFd, unix::net::UnixStream},
        path::Path,
        time::Instant,
    };
    pub fn main() -> Result<()> {
        if unsafe { libc::geteuid() } != 0 {
            return Err("电源程序必须由 root 启动".into());
        }
        let args: Vec<String> = std::env::args().collect();
        let mode = args.get(1).map(String::as_str).unwrap_or("");
        if mode == "configure" {
            let data = fs::canonicalize(args.get(2).ok_or("缺少数据目录")?)?;
            if data == Path::new("/") {
                return Err("无效数据目录".into());
            }
            let uid = args.get(3).ok_or("缺少应用 UID")?.parse::<u32>()?;
            if uid == 0 {
                return Err("应用用户不能是 root".into());
            }
            return system::atomic_write(
                Path::new(system::CONFIG),
                &serde_json::to_vec(&Config { data, uid })?,
            );
        }
        if mode == "cleanup" {
            system::remove_timers(Path::new("/etc/systemd/system"))?;
            return Rtc::default().cleanup();
        }
        let config: Config = serde_json::from_slice(&fs::read(system::CONFIG)?)?;
        let store = Store::open(&config.data.join("scheduler.db"))?;
        match mode {
            "control" => control(&store, config.uid),
            "init" | "boot" => {
                store.db.execute_batch("BEGIN IMMEDIATE")?;
                store.clear_stale_pending(Utc::now())?;
                store.set_meta(
                    "protection_until",
                    &(Utc::now() + Duration::minutes(2)).to_rfc3339(),
                )?;
                system::apply_recorded(&store)?;
                store.db.execute_batch("COMMIT")?;
                if !store.meta("apply_error").is_empty() {
                    eprintln!("计划尚未完全应用：{}", store.meta("apply_error"));
                }
                Ok(())
            }
            "reconcile" => {
                store.db.execute_batch("BEGIN IMMEDIATE")?;
                let result = system::sync_rtc(&store, Utc::now());
                store.set_meta(
                    "rtc_error",
                    &result
                        .as_ref()
                        .err()
                        .map(ToString::to_string)
                        .unwrap_or_default(),
                )?;
                store.db.execute_batch("COMMIT")?;
                result.map(|_| ())
            }
            "execute" => execute(&store, args.get(2).ok_or("缺少任务 ID")?),
            "cancel-stop" => {
                store.db.execute_batch("BEGIN IMMEDIATE")?;
                if store.pending()?.is_some() {
                    store.cancel(Utc::now())?;
                }
                store.db.execute_batch("COMMIT")?;
                Ok(())
            }
            _ => Err("无效电源程序操作".into()),
        }
    }
    fn control(store: &Store, uid: u32) -> Result<()> {
        // Accept=yes + StandardInput=socket supplies an accepted socket as fd 0.
        let mut stream = unsafe { UnixStream::from_raw_fd(0) };
        stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
        stream.set_write_timeout(Some(std::time::Duration::from_secs(5)))?;
        if peer_uid(&stream)? != uid {
            return Err("电源接口调用者无权限".into());
        }
        let mut bytes = Vec::new();
        loop {
            let mut b = [0];
            stream.read_exact(&mut b)?;
            if b[0] == b'\n' {
                break;
            }
            bytes.push(b[0]);
            if bytes.len() > 16384 {
                return Err("请求过大".into());
            }
        }
        let result = (|| -> Result<Value> {
            let req: Value = serde_json::from_slice(&bytes)?;
            if req["operation"] == "state" {
                return store.state_view(Utc::now(), Rtc::default().status());
            }
            store.db.execute_batch("BEGIN IMMEDIATE")?;
            store.recover_expired_pending(Utc::now())?;
            let value = match req["operation"].as_str().ok_or("缺少操作")? {
                "save" => {
                    store.save(serde_json::from_value(req["task"].clone())?, Utc::now())?;
                    store.set_meta(
                        "apply_error",
                        "计划正在应用；若长时间未完成，请重新保存计划",
                    )?;
                    store.db.execute_batch("COMMIT; BEGIN IMMEDIATE")?;
                    system::apply_recorded(store)?;
                    json!({"ok":true})
                }
                "delete" => {
                    store.delete(req["id"].as_str().ok_or("缺少 ID")?)?;
                    store.set_meta(
                        "apply_error",
                        "计划正在应用；若长时间未完成，请重新保存计划",
                    )?;
                    store.db.execute_batch("COMMIT; BEGIN IMMEDIATE")?;
                    system::apply_recorded(store)?;
                    json!({"ok":true})
                }
                "cancel" => {
                    store.cancel(Utc::now())?;
                    json!({"ok":true})
                }
                _ => return Err("不允许的操作".into()),
            };
            store.db.execute_batch("COMMIT")?;
            Ok(value)
        })();
        if result.is_err() {
            let _ = store.db.execute_batch("ROLLBACK");
        }
        let response = match result {
            Ok(v) => json!({"value":v}),
            Err(e) => json!({"error":e.to_string()}),
        };
        writeln!(stream, "{response}")?;
        Ok(())
    }
    fn execute(store: &Store, id: &str) -> Result<()> {
        store.db.execute_batch("BEGIN IMMEDIATE")?;
        let now = Utc::now();
        let token = store.begin_countdown(id, now, system::protected(store, now))?;
        store.db.execute_batch("COMMIT")?;
        let Some(token) = token else {
            return Ok(());
        };
        let started = Instant::now();
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
            let Some(p) = store.pending()? else {
                return Ok(());
            };
            if p["token"] != token {
                return Ok(());
            }
            let wall = (Utc::now() - now).num_milliseconds();
            let elapsed = started.elapsed().as_millis() as i64;
            if (wall - elapsed).abs() > 3000 {
                return finish_failure(store, &token, "系统时间变化，取消本次关机");
            }
            if elapsed >= 60000 {
                break;
            }
        }
        store.db.execute_batch("BEGIN IMMEDIATE")?;
        let Some(p) = store.pending()? else {
            store.db.execute_batch("COMMIT")?;
            return Ok(());
        };
        if p["token"] != token {
            store.db.execute_batch("COMMIT")?;
            return Ok(());
        }
        let t: Task = serde_json::from_value(p["task"].clone())?;
        if !store
            .tasks()?
            .iter()
            .any(|task| task.id == t.id && task.enabled && task.action == "shutdown")
        {
            store.record(&t, "skipped", "任务已禁用或删除", Utc::now())?;
        } else {
            let result = (|| -> Result<()> {
                let alarm = system::sync_rtc(store, Utc::now())?;
                if alarm != 0 && alarm <= Utc::now().timestamp() + 120 {
                    return Err("开机时间距离关机不足两分钟，取消关机".into());
                }
                if Rtc::default().read()? != alarm
                    || (alarm != 0 && Rtc::default().owned()? != alarm)
                {
                    return Err("关机前 RTC 状态改变，取消关机".into());
                }
                // Persist request before asking systemd to stop the machine.
                store.record(&t, "requested", "已请求正常关机", Utc::now())?;
                Ok(())
            })();
            match result {
                Err(e) => store.record(&t, "failed", &e.to_string(), Utc::now())?,
                Ok(()) => {
                    store.db.execute("DELETE FROM pending", [])?;
                    store.db.execute_batch("COMMIT")?;
                    if let Err(e) =
                        system::command("/usr/bin/systemctl", &["--no-block", "poweroff"])
                    {
                        store.record(&t, "failed", &e.to_string(), Utc::now())?;
                        return Err(e);
                    }
                    return Ok(());
                }
            }
        }
        store.db.execute("DELETE FROM pending", [])?;
        store.db.execute_batch("COMMIT")?;
        Ok(())
    }
    fn finish_failure(store: &Store, token: &str, message: &str) -> Result<()> {
        store.db.execute_batch("BEGIN IMMEDIATE")?;
        if let Some(p) = store.pending()?
            && p["token"] == token
        {
            let t: Task = serde_json::from_value(p["task"].clone())?;
            store.record(&t, "skipped", message, Utc::now())?;
            store.db.execute("DELETE FROM pending", [])?;
        }
        store.db.execute_batch("COMMIT")?;
        Ok(())
    }
}
fn main() {
    #[cfg(target_os = "linux")]
    if let Err(e) = linux::main() {
        eprintln!("{e}");
        std::process::exit(1);
    }
    #[cfg(not(target_os = "linux"))]
    {
        eprintln!("电源操作仅支持 Linux；本地请运行 server 模拟模式");
        std::process::exit(1);
    }
}
