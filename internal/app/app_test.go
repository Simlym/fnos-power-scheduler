package app

import (
	"errors"
	"fnos-power-scheduler/internal/power"
	"fnos-power-scheduler/internal/schedule"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
	"time"
)

type fakePower struct {
	calls []power.Request
	fail  bool
}

func TestDatabaseOpenErrorIncludesPath(t *testing.T) {
	path := filepath.Join(t.TempDir(), "missing", "scheduler.db")
	a, err := Open(path, &fakePower{})
	if a != nil {
		a.Close()
		t.Fatal("unexpected database opened")
	}
	if err == nil || !strings.Contains(err.Error(), strconv.Quote(path)) || !strings.Contains(err.Error(), "读写权限") {
		t.Fatalf("missing database context: %v", err)
	}
}

func (f *fakePower) Do(r power.Request) (power.Status, error) {
	f.calls = append(f.calls, r)
	if f.fail && (r.Operation == "set" || r.Operation == "clear") {
		return power.Status{}, errors.New("RTC unavailable")
	}
	return power.Status{Mode: "simulation", Available: true, Alarm: r.Alarm}, nil
}
func fixture(t *testing.T) (*App, *fakePower, time.Time) {
	t.Helper()
	p := &fakePower{}
	a, err := Open(filepath.Join(t.TempDir(), "test.db"), p)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { a.Close() })
	now := time.Date(2026, 10, 9, 15, 0, 0, 0, time.UTC)
	a.clock = func() time.Time { return now }
	a.last = now.Add(-time.Second)
	a.protection = now.Add(-time.Minute)
	return a, p, now
}
func addShutdown(t *testing.T, a *App) {
	t.Helper()
	err := a.Save(schedule.Task{Name: "关机", Action: "shutdown", Repeat: "daily", Time: "23:00", Timezone: "Asia/Shanghai", Enabled: true})
	if err != nil {
		t.Fatal(err)
	}
}
func hasPoweroff(p *fakePower) bool {
	for _, r := range p.calls {
		if r.Operation == "poweroff" {
			return true
		}
	}
	return false
}
func TestCancelAndDedup(t *testing.T) {
	a, p, now := fixture(t)
	addShutdown(t, a)
	a.Tick(now)
	if a.pending == nil {
		t.Fatal("missing countdown")
	}
	if err := a.Cancel(); err != nil {
		t.Fatal(err)
	}
	for i := 1; i <= 61; i++ {
		a.Tick(now.Add(time.Duration(i) * time.Second))
	}
	if hasPoweroff(p) {
		t.Fatal("cancelled occurrence fired")
	}
	if a.pending != nil {
		t.Fatal("same occurrence retriggered")
	}
}
func TestProtectionAndMissedTask(t *testing.T) {
	a, p, now := fixture(t)
	addShutdown(t, a)
	a.protection = now.Add(2 * time.Minute)
	a.Tick(now)
	if a.pending != nil {
		t.Fatal("startup protection failed")
	}
	a.protection = now.Add(-time.Minute)
	a.last = now.Add(-5 * time.Minute)
	a.Tick(now.Add(24 * time.Hour))
	if hasPoweroff(p) || a.pending != nil {
		t.Fatal("missed occurrence caught up")
	}
}
func TestCountdownExecutionAndClockJump(t *testing.T) {
	a, p, now := fixture(t)
	addShutdown(t, a)
	a.Tick(now)
	for i := 1; i <= 60; i++ {
		a.Tick(now.Add(time.Duration(i) * time.Second))
	}
	if !hasPoweroff(p) {
		t.Fatal("shutdown not requested after countdown")
	}
	p.calls = nil
	a.Tick(now.Add(24 * time.Hour))
	if hasPoweroff(p) {
		t.Fatal("long gap triggered shutdown")
	}
}
func TestOccurrenceIsPersisted(t *testing.T) {
	a, p, now := fixture(t)
	addShutdown(t, a)
	a.Tick(now)
	a.Cancel()
	a.tasks = nil
	rows, err := a.db.Query("SELECT COUNT(*) FROM occurrences")
	if err != nil {
		t.Fatal(err)
	}
	defer rows.Close()
	var n int
	if rows.Next() {
		rows.Scan(&n)
	}
	if n != 1 || hasPoweroff(p) {
		t.Fatal("occurrence was not persisted")
	}
}

func TestRTCFailureCancelsShutdown(t *testing.T) {
	a, p, now := fixture(t)
	addShutdown(t, a)
	a.Tick(now)
	p.fail = true
	for i := 1; i <= 60; i++ {
		a.Tick(now.Add(time.Duration(i) * time.Second))
	}
	if hasPoweroff(p) || a.pending != nil {
		t.Fatal("RTC failure must cancel shutdown")
	}
	view, err := a.View()
	if err != nil {
		t.Fatal(err)
	}
	if len(view.Records) == 0 || view.Records[0].Status != "failed" {
		t.Fatal("failure must be recorded")
	}
}

func TestReopenPreservesRulesAndDeduplication(t *testing.T) {
	path := filepath.Join(t.TempDir(), "restart.db")
	p := &fakePower{}
	a, err := Open(path, p)
	if err != nil {
		t.Fatal(err)
	}
	now := time.Date(2026, 10, 9, 15, 0, 0, 0, time.UTC)
	a.clock = func() time.Time { return now }
	a.last = now.Add(-time.Second)
	a.protection = now.Add(-time.Minute)
	addShutdown(t, a)
	a.Tick(now)
	if err = a.Cancel(); err != nil {
		t.Fatal(err)
	}
	a.Close()
	restarted, err := Open(path, p)
	if err != nil {
		t.Fatal(err)
	}
	defer restarted.Close()
	restarted.clock = func() time.Time { return now }
	restarted.last = now
	restarted.protection = now.Add(-time.Minute)
	restarted.Tick(now.Add(time.Second))
	if len(restarted.tasks) != 1 || restarted.pending != nil {
		t.Fatal("restart lost rules or repeated processed occurrence")
	}
}

func TestBackwardsClockCancelsCountdown(t *testing.T) {
	a, p, now := fixture(t)
	addShutdown(t, a)
	a.Tick(now)
	a.Tick(now.Add(-time.Second))
	if a.pending != nil || hasPoweroff(p) {
		t.Fatal("backwards clock did not cancel countdown")
	}
}
