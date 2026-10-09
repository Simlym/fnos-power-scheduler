package app

import (
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"fnos-power-scheduler/internal/power"
	"fnos-power-scheduler/internal/schedule"
	_ "modernc.org/sqlite"
	"sync"
	"time"
)

type Record struct {
	TaskID  string `json:"taskId"`
	Name    string `json:"name"`
	At      string `json:"at"`
	Status  string `json:"status"`
	Message string `json:"message"`
}
type Pending struct {
	TaskID     string    `json:"taskId"`
	Name       string    `json:"name"`
	Deadline   time.Time `json:"deadline"`
	Occurrence time.Time `json:"-"`
}
type View struct {
	Tasks           []schedule.Task       `json:"tasks"`
	Records         []Record              `json:"records"`
	Next            map[string]*time.Time `json:"next"`
	Pending         *Pending              `json:"pending"`
	Power           power.Status          `json:"power"`
	ProtectionUntil time.Time             `json:"protectionUntil"`
	Now             time.Time             `json:"now"`
	Error           string                `json:"error"`
}
type App struct {
	mu         sync.Mutex
	db         *sql.DB
	tasks      []schedule.Task
	controller power.Controller
	pending    *Pending
	protection time.Time
	last       time.Time
	rtcError   string
	status     power.Status
	desired    int64
	synced     bool
	clock      func() time.Time
}

func Open(path string, controller power.Controller) (*App, error) {
	db, err := sql.Open("sqlite", path)
	if err != nil {
		return nil, err
	}
	db.SetMaxOpenConns(1)
	_, err = db.Exec(`PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;
 CREATE TABLE IF NOT EXISTS tasks(id TEXT PRIMARY KEY, body TEXT NOT NULL);
 CREATE TABLE IF NOT EXISTS records(id INTEGER PRIMARY KEY,task_id TEXT,name TEXT,at TEXT,status TEXT,message TEXT);
 CREATE TABLE IF NOT EXISTS occurrences(task_id TEXT,at TEXT,PRIMARY KEY(task_id,at));`)
	if err != nil {
		db.Close()
		return nil, err
	}
	a := &App{db: db, controller: controller, tasks: []schedule.Task{}, clock: time.Now, protection: time.Now().Add(2 * time.Minute), last: time.Now()}
	rows, err := db.Query("SELECT body FROM tasks ORDER BY id")
	if err != nil {
		db.Close()
		return nil, err
	}
	defer rows.Close()
	for rows.Next() {
		var body string
		if err = rows.Scan(&body); err != nil {
			return nil, err
		}
		var t schedule.Task
		if err = json.Unmarshal([]byte(body), &t); err != nil {
			return nil, err
		}
		if err = t.Validate(); err != nil {
			return nil, err
		}
		a.tasks = append(a.tasks, t)
	}
	if err = rows.Err(); err != nil {
		return nil, err
	}
	return a, nil
}
func (a *App) Close() error { return a.db.Close() }
func (a *App) record(t schedule.Task, at time.Time, status, message string) error {
	_, err := a.db.Exec("INSERT INTO records(task_id,name,at,status,message) VALUES(?,?,?,?,?)", t.ID, t.Name, at.Format(time.RFC3339), status, message)
	return err
}
func (a *App) nextWake(now time.Time) int64 {
	var alarm int64
	for _, t := range a.tasks {
		if t.Action == "wake" {
			n := t.Next(now)
			if n != nil && (alarm == 0 || n.Unix() < alarm) {
				alarm = n.Unix()
			}
		}
	}
	return alarm
}
func (a *App) syncRTC(now time.Time, force bool) error {
	alarm := a.nextWake(now)
	if !force && a.synced && alarm == a.desired {
		return nil
	}
	operation := "set"
	if alarm == 0 {
		operation = "clear"
	}
	s, err := a.controller.Do(power.Request{Operation: operation, Alarm: alarm})
	a.status = s
	if err != nil {
		a.synced = false
		a.rtcError = err.Error()
		return err
	}
	a.desired = alarm
	a.synced = true
	a.rtcError = ""
	return nil
}
func (a *App) Save(t schedule.Task) error {
	a.mu.Lock()
	defer a.mu.Unlock()
	if err := t.Validate(); err != nil {
		return err
	}
	now := a.clock()
	if t.Enabled && t.Repeat == "once" && t.Next(now) == nil {
		return errors.New("单次任务时间必须在未来")
	}
	if a.pending != nil && a.pending.TaskID == t.ID {
		return errors.New("请先取消正在倒计时的任务")
	}
	for _, other := range a.tasks {
		if other.ID != t.ID && schedule.Conflict(t, other, now) {
			return fmt.Errorf("与任务“%s”的开关机时间冲突", other.Name)
		}
	}
	found := false
	for _, other := range a.tasks {
		if other.ID == t.ID {
			found = true
		}
	}
	if t.ID != "" && !found {
		return errors.New("任务不存在")
	}
	if t.ID == "" {
		t.ID = schedule.NewID()
	}
	data, _ := json.Marshal(t)
	if _, err := a.db.Exec("INSERT INTO tasks(id,body) VALUES(?,?) ON CONFLICT(id) DO UPDATE SET body=excluded.body", t.ID, string(data)); err != nil {
		return err
	}
	if found {
		for i := range a.tasks {
			if a.tasks[i].ID == t.ID {
				a.tasks[i] = t
			}
		}
	} else {
		a.tasks = append(a.tasks, t)
	}
	// Persist the rule even if hardware programming fails; report RTC failure in state.
	a.syncRTC(now, true)
	return nil
}
func (a *App) Delete(id string) error {
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.pending != nil && a.pending.TaskID == id {
		return errors.New("请先取消正在倒计时的任务")
	}
	result, err := a.db.Exec("DELETE FROM tasks WHERE id=?", id)
	if err != nil {
		return err
	}
	count, _ := result.RowsAffected()
	if count == 0 {
		return errors.New("任务不存在")
	}
	for i, t := range a.tasks {
		if t.ID == id {
			a.tasks = append(a.tasks[:i], a.tasks[i+1:]...)
			break
		}
	}
	a.syncRTC(a.clock(), true)
	return nil
}
func (a *App) Cancel() error {
	a.mu.Lock()
	defer a.mu.Unlock()
	if a.pending == nil {
		return errors.New("没有待取消的关机")
	}
	p := a.pending
	if err := a.record(schedule.Task{ID: p.TaskID, Name: p.Name}, a.clock(), "cancelled", "用户取消本次关机"); err != nil {
		return err
	}
	a.pending = nil
	return nil
}
func (a *App) Tick(now time.Time) {
	a.mu.Lock()
	defer a.mu.Unlock()
	// Compare wall time, not Go's monotonic clock, to detect NTP/manual clock jumps.
	now = now.Round(0)
	a.last = a.last.Round(0)
	a.syncRTC(now, false)
	minute := now.Truncate(time.Minute)
	// A delay exceeding a minute, or a backwards clock jump, never catches up a shutdown.
	contiguous := !now.Before(a.last) && now.Sub(a.last) <= time.Minute
	if contiguous {
		for _, t := range a.tasks {
			if !t.Enabled || t.Action != "shutdown" || !t.Matches(minute) {
				continue
			}
			location, _ := time.LoadLocation(t.Timezone)
			// A repeated local minute at the autumn DST transition is one occurrence.
			occurrenceKey := minute.In(location).Format("2006-01-02T15:04") + "@" + t.Timezone
			result, err := a.db.Exec("INSERT OR IGNORE INTO occurrences(task_id,at) VALUES(?,?)", t.ID, occurrenceKey)
			if err != nil {
				a.rtcError = err.Error()
				continue
			}
			count, _ := result.RowsAffected()
			if count == 0 {
				continue
			}
			if now.Before(a.protection) {
				a.record(t, now, "skipped", "应用启动保护期，跳过本次关机")
				continue
			}
			if a.pending != nil {
				a.record(t, now, "skipped", "已有任务正在倒计时")
				continue
			}
			if err = a.record(t, now, "countdown", "进入 60 秒关机倒计时"); err != nil {
				a.rtcError = err.Error()
				continue
			}
			a.pending = &Pending{TaskID: t.ID, Name: t.Name, Deadline: now.Add(time.Minute), Occurrence: minute}
		}
	}
	if a.pending != nil && !now.Before(a.pending.Deadline) {
		p := a.pending
		a.pending = nil
		t := schedule.Task{ID: p.TaskID, Name: p.Name}
		if !contiguous {
			a.record(t, now, "skipped", "调度中断或系统时间变化，取消本次关机")
		} else if err := a.syncRTC(now, true); err != nil {
			a.record(t, now, "failed", "RTC 核对失败，取消关机："+err.Error())
		} else if err := a.record(t, now, "requested", "已请求正常关机"); err != nil {
			a.rtcError = err.Error()
		} else if _, err := a.controller.Do(power.Request{Operation: "poweroff", Alarm: a.desired}); err != nil {
			a.record(t, now, "failed", err.Error())
		}
	}
	// Backwards changes invalidate pending countdowns.
	if now.Before(a.last) && a.pending != nil {
		p := a.pending
		a.pending = nil
		a.record(schedule.Task{ID: p.TaskID, Name: p.Name}, now, "skipped", "系统时间回拨，取消本次关机")
	}
	a.last = now
}
func (a *App) View() (View, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	now := a.clock()
	s, err := a.controller.Do(power.Request{Operation: "status"})
	if err != nil {
		s = power.Status{Mode: "hardware", Message: err.Error()}
	}
	v := View{Tasks: append([]schedule.Task{}, a.tasks...), Records: []Record{}, Next: map[string]*time.Time{}, Power: s, ProtectionUntil: a.protection, Now: now, Error: a.rtcError}
	if a.pending != nil {
		p := *a.pending
		v.Pending = &p
	}
	for _, t := range a.tasks {
		v.Next[t.ID] = t.Next(now)
	}
	rows, err := a.db.Query("SELECT task_id,name,at,status,message FROM records ORDER BY id DESC LIMIT 100")
	if err != nil {
		return v, err
	}
	defer rows.Close()
	for rows.Next() {
		var r Record
		if err = rows.Scan(&r.TaskID, &r.Name, &r.At, &r.Status, &r.Message); err != nil {
			return v, err
		}
		v.Records = append(v.Records, r)
	}
	return v, rows.Err()
}
