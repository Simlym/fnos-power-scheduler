package schedule

import (
	"testing"
	"time"
)

func TestNextWeeklyAndOnce(t *testing.T) {
	now := time.Date(2026, 10, 9, 16, 0, 0, 0, time.UTC)
	task := Task{Name: "早间开机", Action: "wake", Repeat: "weekly", Time: "08:00", Weekdays: []int{1}, Timezone: "Asia/Shanghai", Enabled: true}
	got := task.Next(now)
	want := time.Date(2026, 10, 12, 0, 0, 0, 0, time.UTC)
	if got == nil || !got.Equal(want) {
		t.Fatalf("next=%v want=%v", got, want)
	}
	task.Repeat = "once"
	task.Date = "2027-01-01"
	got = task.Next(now)
	want = time.Date(2027, 1, 1, 0, 0, 0, 0, time.UTC)
	if got == nil || !got.Equal(want) {
		t.Fatalf("distant once=%v", got)
	}
	if task.Next(want) != nil {
		t.Fatal("one-off occurrence must not repeat")
	}
}
func TestDSTGap(t *testing.T) {
	task := Task{Name: "DST", Action: "wake", Repeat: "daily", Time: "02:30", Timezone: "America/New_York", Enabled: true}
	now := time.Date(2026, 3, 8, 5, 0, 0, 0, time.UTC)
	got := task.Next(now)
	want := time.Date(2026, 3, 9, 6, 30, 0, 0, time.UTC)
	if got == nil || !got.Equal(want) {
		t.Fatalf("DST gap should skip nonexistent time: %v", got)
	}
}
func TestConflictDistantOnceAndDifferentZones(t *testing.T) {
	a := Task{Name: "wake", Action: "wake", Repeat: "once", Date: "2027-01-01", Time: "08:00", Timezone: "Asia/Shanghai", Enabled: true}
	b := Task{Name: "off", Action: "shutdown", Repeat: "daily", Time: "00:00", Timezone: "UTC", Enabled: true}
	if !Conflict(a, b, time.Date(2026, 10, 9, 0, 0, 0, 0, time.UTC)) {
		t.Fatal("must detect distant cross-zone conflict")
	}
	b.Enabled = false
	if Conflict(a, b, time.Now()) {
		t.Fatal("disabled rules must not conflict")
	}
}
