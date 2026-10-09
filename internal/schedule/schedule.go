package schedule

import (
	"errors"
	"fmt"
	"strconv"
	"strings"
	"time"
	_ "time/tzdata"
)

type Task struct {
	ID       string `json:"id"`
	Name     string `json:"name"`
	Action   string `json:"action"`
	Repeat   string `json:"repeat"`
	Time     string `json:"time"`
	Date     string `json:"date"`
	Weekdays []int  `json:"weekdays"`
	Timezone string `json:"timezone"`
	Enabled  bool   `json:"enabled"`
}

func (t Task) Validate() error {
	if len(strings.TrimSpace(t.Name)) == 0 || len(t.Name) > 120 {
		return errors.New("任务名称不能为空，且不能超过 120 字节")
	}
	if t.Action != "shutdown" && t.Action != "wake" {
		return errors.New("不支持的电源操作")
	}
	if t.Repeat != "once" && t.Repeat != "daily" && t.Repeat != "weekly" {
		return errors.New("不支持的重复规则")
	}
	if _, err := time.Parse("15:04", t.Time); err != nil {
		return errors.New("时间必须为 HH:mm")
	}
	if _, err := time.LoadLocation(t.Timezone); err != nil {
		return errors.New("无效的时区")
	}
	if t.Repeat == "once" {
		if _, err := time.Parse("2006-01-02", t.Date); err != nil {
			return errors.New("单次任务需要有效日期")
		}
	}
	if t.Repeat == "weekly" && len(t.Weekdays) == 0 {
		return errors.New("请选择星期")
	}
	seen := map[int]bool{}
	for _, d := range t.Weekdays {
		if d < 0 || d > 6 || seen[d] {
			return errors.New("星期必须为不重复的 0 至 6")
		}
		seen[d] = true
	}
	return nil
}

func (t Task) Matches(at time.Time) bool {
	loc, err := time.LoadLocation(t.Timezone)
	if err != nil {
		return false
	}
	local := at.In(loc)
	if local.Format("15:04") != t.Time {
		return false
	}
	switch t.Repeat {
	case "daily":
		return true
	case "once":
		return local.Format("2006-01-02") == t.Date
	case "weekly":
		for _, d := range t.Weekdays {
			if int(local.Weekday()) == d {
				return true
			}
		}
	}
	return false
}

// Next is strictly after its argument. Iterating absolute minutes handles DST gaps.
func (t Task) Next(after time.Time) *time.Time {
	if !t.Enabled || t.Validate() != nil {
		return nil
	}
	start := after.Truncate(time.Minute).Add(time.Minute)
	if t.Repeat == "once" {
		loc, _ := time.LoadLocation(t.Timezone)
		day, _ := time.ParseInLocation("2006-01-02", t.Date, loc)
		if day.AddDate(0, 0, 2).Before(start) {
			return nil
		}
		if day.After(start) {
			start = day.Add(-time.Hour)
		}
	}
	for i := 0; i < 9*24*60; i++ {
		at := start.Add(time.Duration(i) * time.Minute)
		if t.Matches(at) {
			return &at
		}
	}
	return nil
}

func Conflict(a, b Task, now time.Time) bool {
	if !a.Enabled || !b.Enabled || a.Action == b.Action {
		return false
	}
	start := now
	// Include distant one-off schedules, rather than only looking at this week.
	for _, t := range []Task{a, b} {
		if t.Repeat == "once" {
			loc, _ := time.LoadLocation(t.Timezone)
			day, _ := time.ParseInLocation("2006-01-02", t.Date, loc)
			if day.After(start) {
				start = day.Add(-time.Minute)
			}
		}
	}
	x, y := a.Next(start), b.Next(start)
	limit := start.AddDate(0, 0, 9)
	for x != nil && y != nil && x.Before(limit) && y.Before(limit) {
		if x.Equal(*y) {
			return true
		}
		if x.Before(*y) {
			x = a.Next(*x)
		} else {
			y = b.Next(*y)
		}
	}
	return false
}

func NewID() string { return fmt.Sprintf("%s", strconv.FormatInt(time.Now().UnixNano(), 36)) }
