use crate::{Result, schedule::Task, store::Store};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub const CONFIG: &str = "/var/lib/fnos-power-scheduler/rust-config.json";
pub const EXE: &str = "/usr/local/libexec/fnos-power-scheduler/powerd";
const PREFIX: &str = "fnps-rust-plan-";
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub data: PathBuf,
    pub uid: u32,
}
pub fn command(program: &str, args: &[&str]) -> Result<()> {
    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return if status.success() {
                Ok(())
            } else {
                Err(format!("{program} 执行失败：{status}").into())
            };
        }
        if start.elapsed() > Duration::from_secs(10) {
            child.kill()?;
            let _ = child.wait();
            return Err(format!("{program} 执行超时").into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
pub fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    use std::io::Write;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| -> Result<()> {
        let mut f = options.open(&temporary)?;
        f.write_all(contents)?;
        f.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if temporary.exists() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
pub struct Rtc {
    pub alarm: PathBuf,
    pub ownership: PathBuf,
}
impl Default for Rtc {
    fn default() -> Self {
        Self {
            alarm: "/sys/class/rtc/rtc0/wakealarm".into(),
            ownership: "/var/lib/fnos-power-scheduler/owned-alarm".into(),
        }
    }
}
impl Rtc {
    fn number(path: &Path) -> Result<i64> {
        let s = fs::read_to_string(path)?;
        if s.trim().is_empty() {
            Ok(0)
        } else {
            Ok(s.trim().parse()?)
        }
    }
    pub fn read(&self) -> Result<i64> {
        Self::number(&self.alarm)
    }
    pub fn owned(&self) -> Result<i64> {
        if self.ownership.exists() {
            Self::number(&self.ownership)
        } else {
            Ok(0)
        }
    }
    pub fn set(&self, target: i64, now: DateTime<Utc>) -> Result<()> {
        if self.alarm == Path::new("/sys/class/rtc/rtc0/wakealarm")
            && fs::read_to_string("/etc/adjtime")
                .ok()
                .is_some_and(|s| s.lines().nth(2) == Some("LOCAL"))
        {
            return Err("当前 RTC 使用本地时间，此版本只支持 UTC RTC；未写入闹钟".into());
        }
        let current = self.read()?;
        let owned = self.owned()?;
        if current != 0 && current != owned {
            return Err("RTC 闹钟由其他程序设置，未覆盖".into());
        }
        if target != 0 && (target <= now.timestamp() + 5 || target > now.timestamp() + 366 * 86400)
        {
            return Err("RTC 时间必须在未来 5 秒至一年之间".into());
        }
        if current != target {
            fs::write(&self.alarm, "0")?;
            if target != 0 {
                fs::write(&self.alarm, target.to_string())?;
            }
            if self.read()? != target {
                return Err("RTC 写入后读回不一致".into());
            }
        }
        atomic_write(&self.ownership, target.to_string().as_bytes())?;
        Ok(())
    }
    pub fn status(&self) -> Value {
        match self.read() {
            Ok(alarm) => {
                json!({"mode":"hardware","available":true,"alarm":alarm,"message":"RTC 接口可用"})
            }
            Err(e) => {
                json!({"mode":"hardware","available":false,"alarm":0,"message":e.to_string()})
            }
        }
    }
    pub fn cleanup(&self) -> Result<()> {
        let owned = self.owned()?;
        if owned != 0 && self.read()? == owned {
            fs::write(&self.alarm, "0")?;
        }
        if self.ownership.exists() {
            fs::remove_file(&self.ownership)?;
        }
        Ok(())
    }
}
pub fn next_wake(tasks: &[Task], now: DateTime<Utc>) -> i64 {
    tasks
        .iter()
        .filter(|t| t.action == "wake")
        .filter_map(|t| t.next(now))
        .map(|t| t.timestamp())
        .min()
        .unwrap_or(0)
}
pub fn sync_rtc(store: &Store, now: DateTime<Utc>) -> Result<i64> {
    let target = next_wake(&store.tasks()?, now);
    Rtc::default().set(target, now)?;
    Ok(target)
}
pub fn unit(task: &Task) -> String {
    let command = if task.action == "shutdown" {
        format!("execute {}", task.id)
    } else {
        "reconcile".into()
    };
    format!(
        "[Unit]\nDescription=Power schedule {}\nRequires=fnps-rust-lifecycle.service\nAfter=fnps-rust-lifecycle.service\n\n[Service]\nType=oneshot\nExecStart={EXE} {command}\nTimeoutStartSec=90\nUMask=0077\nNoNewPrivileges=yes\n",
        task.id
    )
}
pub fn timer(task: &Task) -> String {
    format!(
        "[Unit]\nDescription=Power schedule {}\n\n[Timer]\nOnCalendar={}\nAccuracySec=1s\nRandomizedDelaySec=0\nPersistent=false\nUnit={PREFIX}{}.service\n\n[Install]\nWantedBy=timers.target\n",
        task.id,
        task.calendar(),
        task.id
    )
}
pub fn remove_timers(directory: &Path) -> Result<()> {
    remove_timers_with(directory, &mut command)
}
fn remove_timers_with(
    directory: &Path,
    run: &mut impl FnMut(&str, &[&str]) -> Result<()>,
) -> Result<()> {
    let mut files = Vec::new();
    let mut timers = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with(PREFIX) || !(name.ends_with(".timer") || name.ends_with(".service")) {
            continue;
        }
        if entry.file_type()?.is_symlink() || !entry.file_type()?.is_file() {
            return Err("计划配置路径异常".into());
        }
        if name.ends_with(".timer") {
            timers.push(name);
        }
        files.push(entry.path());
    }
    if !timers.is_empty() {
        let mut args = vec!["disable", "--now"];
        args.extend(timers.iter().map(String::as_str));
        run("/usr/bin/systemctl", &args)?;
    }
    for file in files {
        fs::remove_file(file)?;
    }
    Ok(())
}
pub fn apply(store: &Store) -> Result<()> {
    let tasks = store.tasks()?;
    apply_timers(&tasks, Path::new("/etc/systemd/system"), &mut command)?;
    sync_rtc(store, Utc::now())?;
    Ok(())
}
fn apply_timers(
    tasks: &[Task],
    dir: &Path,
    run: &mut impl FnMut(&str, &[&str]) -> Result<()>,
) -> Result<()> {
    // Disable old timers first. A failed rebuild cannot silently run obsolete rules.
    remove_timers_with(dir, run)?;
    for t in tasks.iter().filter(|t| t.enabled) {
        atomic_write(
            &dir.join(format!("{PREFIX}{}.service", t.id)),
            unit(t).as_bytes(),
        )?;
        atomic_write(
            &dir.join(format!("{PREFIX}{}.timer", t.id)),
            timer(t).as_bytes(),
        )?;
    }
    run("/usr/bin/systemctl", &["daemon-reload"])?;
    let timers: Vec<String> = tasks
        .iter()
        .filter(|t| t.enabled)
        .map(|t| format!("{PREFIX}{}.timer", t.id))
        .collect();
    if !timers.is_empty() {
        let mut args = vec!["enable", "--now"];
        args.extend(timers.iter().map(String::as_str));
        run("/usr/bin/systemctl", &args)?;
    }
    Ok(())
}
pub fn apply_recorded(store: &Store) -> Result<()> {
    match apply(store) {
        Ok(()) => {
            store.set_meta("rtc_error", "")?;
            store.set_meta("apply_error", "")
        }
        Err(e) => {
            store.set_meta("apply_error", &e.to_string())?;
            Ok(())
        }
    }
}
pub fn protected(store: &Store, now: DateTime<Utc>) -> bool {
    let application = store
        .meta("protection_until")
        .parse::<DateTime<Utc>>()
        .is_ok_and(|t| now < t);
    let boot = fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| {
            s.split_whitespace()
                .next()
                .and_then(|t| t.parse::<f64>().ok())
        })
        .is_none_or(|t| t < 120.0);
    application || boot
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn applying_many_plans_batches_systemctl_and_excludes_disabled_tasks() {
        let dir = tempfile::tempdir().unwrap();
        let tasks: Vec<Task> = (0..20)
            .map(|i| {
                serde_json::from_value(json!({
                    "id":format!("task{i}"),"name":"plan","action":"shutdown",
                    "repeat":"daily","time":"23:00","timezone":"Asia/Shanghai",
                    "enabled":i != 0,
                }))
                .unwrap()
            })
            .collect();
        for t in &tasks {
            fs::write(dir.path().join(format!("{PREFIX}{}.timer", t.id)), "old").unwrap();
            fs::write(dir.path().join(format!("{PREFIX}{}.service", t.id)), "old").unwrap();
        }
        fs::write(dir.path().join("other.timer"), "untouched").unwrap();
        let mut calls = Vec::new();
        apply_timers(&tasks, dir.path(), &mut |program, args| {
            assert_eq!(program, "/usr/bin/systemctl");
            calls.push(args.iter().map(|s| s.to_string()).collect::<Vec<_>>());
            Ok(())
        })
        .unwrap();
        assert_eq!(calls.len(), 3);
        assert_eq!(&calls[0][..2], &["disable", "--now"]);
        assert_eq!(calls[0].len(), 22);
        assert_eq!(calls[1], ["daemon-reload"]);
        assert_eq!(&calls[2][..2], &["enable", "--now"]);
        assert_eq!(calls[2].len(), 21);
        assert!(!calls[2].contains(&format!("{PREFIX}task0.timer")));
        assert!(!dir.path().join(format!("{PREFIX}task0.timer")).exists());
        assert_eq!(
            fs::read_to_string(dir.path().join("other.timer")).unwrap(),
            "untouched"
        );
        assert!(
            fs::read_to_string(dir.path().join(format!("{PREFIX}task1.timer")))
                .unwrap()
                .contains("OnCalendar=")
        );
    }
    #[test]
    fn failed_stop_preserves_files_and_aborts_rebuild() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(format!("{PREFIX}old.timer"));
        fs::write(&path, "old").unwrap();
        let mut calls = 0;
        assert!(
            apply_timers(&[], dir.path(), &mut |_, _| {
                calls += 1;
                Err("stop failed".into())
            })
            .is_err()
        );
        assert_eq!(calls, 1);
        assert_eq!(fs::read_to_string(path).unwrap(), "old");
    }
    #[test]
    fn rtc_ownership_and_clearing() {
        let dir = tempfile::tempdir().unwrap();
        let r = Rtc {
            alarm: dir.path().join("alarm"),
            ownership: dir.path().join("owned"),
        };
        fs::write(&r.alarm, "0").unwrap();
        let now = Utc::now();
        let target = now.timestamp() + 600;
        r.set(target, now).unwrap();
        assert_eq!(r.read().unwrap(), target);
        assert_eq!(r.owned().unwrap(), target);
        fs::write(&r.alarm, (target + 100).to_string()).unwrap();
        assert!(r.set(0, now).is_err());
        r.cleanup().unwrap();
        assert_eq!(r.read().unwrap(), target + 100);
    }
    #[test]
    fn daily_multiple_wake_selection() {
        let mut tasks = Vec::new();
        for time in ["08:00", "14:00", "20:00"] {
            tasks.push(serde_json::from_value(json!({"id":"x","name":"wake","action":"wake","repeat":"daily","time":time,"timezone":"Asia/Shanghai","enabled":true})).unwrap());
        }
        let at = |s: &str| s.parse::<DateTime<Utc>>().unwrap();
        assert_eq!(
            next_wake(&tasks, at("2026-10-10T00:00:00Z")),
            at("2026-10-10T06:00:00Z").timestamp()
        );
        assert_eq!(
            next_wake(&tasks, at("2026-10-10T12:00:00Z")),
            at("2026-10-11T00:00:00Z").timestamp()
        );
        assert!(!timer(&tasks[0]).contains("Persistent=true"));
    }
}
