use crate::Result;
use chrono::{DateTime, Datelike, Days, LocalResult, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub action: String,
    pub repeat: String,
    pub time: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub weekdays: Vec<u32>,
    pub timezone: String,
    pub enabled: bool,
}
impl Task {
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() || self.name.len() > 120 {
            return Err("任务名称不能为空且不能超过 120 字节".into());
        }
        if !["wake", "shutdown"].contains(&self.action.as_str()) {
            return Err("无效操作".into());
        }
        if !["once", "daily", "weekly"].contains(&self.repeat.as_str()) {
            return Err("无效重复规则".into());
        }
        if self.time.len() != 5 {
            return Err("时间必须为 HH:mm".into());
        }
        NaiveTime::parse_from_str(&self.time, "%H:%M")?;
        self.timezone.parse::<Tz>()?;
        if self.repeat == "once" {
            NaiveDate::parse_from_str(&self.date, "%Y-%m-%d")?;
        }
        if self.repeat == "weekly" && self.weekdays.is_empty() {
            return Err("请选择星期".into());
        }
        let mut days = self.weekdays.clone();
        days.sort();
        days.dedup();
        if days.len() != self.weekdays.len() || days.iter().any(|d| *d > 6) {
            return Err("无效星期".into());
        }
        if !self.id.is_empty()
            && (self.id.len() > 64
                || !self
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-'))
        {
            return Err("无效任务 ID".into());
        }
        Ok(())
    }
    fn accepts(&self, day: NaiveDate) -> bool {
        match self.repeat.as_str() {
            "once" => day.to_string() == self.date,
            "weekly" => self
                .weekdays
                .contains(&day.weekday().num_days_from_sunday()),
            _ => true,
        }
    }
    pub fn next(&self, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
        if !self.enabled || self.validate().is_err() {
            return None;
        }
        let tz = self.timezone.parse::<Tz>().ok()?;
        let start = if self.repeat == "once" {
            NaiveDate::parse_from_str(&self.date, "%Y-%m-%d").ok()?
        } else {
            after.with_timezone(&tz).date_naive()
        };
        let time = NaiveTime::parse_from_str(&self.time, "%H:%M").ok()?;
        for i in 0..9 {
            let day = start.checked_add_days(Days::new(i))?;
            if !self.accepts(day) {
                continue;
            }
            // Choose the first occurrence of a repeated local minute (DST fall-back).
            let at = match tz.from_local_datetime(&day.and_time(time)) {
                LocalResult::Single(t) => t,
                LocalResult::Ambiguous(a, b) => a.min(b),
                LocalResult::None => continue,
            }
            .with_timezone(&Utc);
            if at > after {
                return Some(at);
            }
            if self.repeat == "once" {
                break;
            }
        }
        None
    }
    pub fn occurrence(&self, now: DateTime<Utc>) -> Option<String> {
        if !self.enabled {
            return None;
        }
        let tz = self.timezone.parse::<Tz>().ok()?;
        let local = now.with_timezone(&tz);
        if !self.accepts(local.date_naive()) || local.format("%H:%M").to_string() != self.time {
            return None;
        }
        Some(format!(
            "{}@{}",
            local.format("%Y-%m-%dT%H:%M"),
            self.timezone
        ))
    }
    pub fn calendar(&self) -> String {
        let day = match self.repeat.as_str() {
            "once" => self.date.clone(),
            "weekly" => {
                let names = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
                format!(
                    "{} *-*-*",
                    self.weekdays
                        .iter()
                        .map(|d| names[*d as usize])
                        .collect::<Vec<_>>()
                        .join(",")
                )
            }
            _ => "*-*-*".into(),
        };
        format!("{day} {}:00 {}", self.time, self.timezone)
    }
}
pub fn conflict(a: &Task, b: &Task, now: DateTime<Utc>) -> bool {
    if !a.enabled || !b.enabled || a.action == b.action {
        return false;
    }
    let mut start = now;
    for t in [a, b] {
        if t.repeat == "once"
            && let Some(at) = t.next(now)
        {
            start = start.max(at - chrono::Duration::days(1));
        }
    }
    let limit = start + chrono::Duration::days(10);
    let (mut x, mut y) = (a.next(start), b.next(start));
    while let (Some(p), Some(q)) = (x, y) {
        if p > limit || q > limit {
            break;
        }
        if p == q {
            return true;
        }
        if p < q {
            x = a.next(p);
        } else {
            y = b.next(q);
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    fn task() -> Task {
        serde_json::from_str(r#"{"name":"test","action":"wake","repeat":"daily","time":"08:00","timezone":"Asia/Shanghai","enabled":true}"#).unwrap()
    }
    fn at(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }
    #[test]
    fn multiple_wakes() {
        let a = task();
        let mut b = task();
        b.time = "14:00".into();
        assert_eq!(
            a.next(at("2026-10-10T00:00:00Z")),
            Some(at("2026-10-11T00:00:00Z"))
        );
        assert_eq!(
            b.next(at("2026-10-10T00:00:00Z")),
            Some(at("2026-10-10T06:00:00Z"))
        );
    }
    #[test]
    fn distant_once_and_conflict() {
        let mut a = task();
        a.repeat = "once".into();
        a.date = "2027-01-01".into();
        let mut b = task();
        b.action = "shutdown".into();
        b.time = "00:00".into();
        b.timezone = "UTC".into();
        assert!(conflict(&a, &b, at("2026-10-10T00:00:00Z")));
        assert_eq!(a.next(at("2027-01-01T00:00:00Z")), None);
    }
    #[test]
    fn dst_gap_and_repeat() {
        let mut t = task();
        t.timezone = "America/New_York".into();
        t.time = "02:30".into();
        assert_eq!(
            t.next(at("2026-03-08T05:00:00Z")),
            Some(at("2026-03-09T06:30:00Z"))
        );
        t.time = "01:30".into();
        assert_eq!(
            t.next(at("2026-11-01T05:30:00Z")),
            Some(at("2026-11-02T06:30:00Z"))
        );
    }
    #[test]
    fn reject_unit_injection() {
        let mut t = task();
        t.timezone = "UTC\nExecStart=/bin/sh".into();
        assert!(t.validate().is_err());
        t = task();
        t.id = "../../bad".into();
        assert!(t.validate().is_err());
    }
}
