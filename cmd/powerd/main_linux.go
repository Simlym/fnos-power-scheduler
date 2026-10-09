//go:build linux

package main

import (
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"fnos-power-scheduler/internal/peer"
	"fnos-power-scheduler/internal/power"
	"log"
	"net"
	"net/http"
	"os"
	"os/exec"
	"os/signal"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"time"
)

type authenticatedListener struct {
	net.Listener
	uid uint32
}

func (l authenticatedListener) Accept() (net.Conn, error) {
	for {
		c, err := l.Listener.Accept()
		if err != nil {
			return nil, err
		}
		uid, err := peer.UID(c)
		if err == nil && uid == l.uid {
			return c, nil
		}
		c.Close()
	}
}
func main() {
	uid := flag.Int("uid", -1, "允许调用的应用用户 UID")
	rtc := flag.String("rtc", "/sys/class/rtc/rtc0/wakealarm", "RTC 闹钟接口")
	cleanup := flag.Bool("cleanup", false, "卸载时清理本应用仍拥有的闹钟")
	flag.Parse()
	if os.Geteuid() != 0 || (!*cleanup && *uid <= 0) {
		log.Fatal("powerd 必须以 root 运行，并指定非 root 应用 uid")
	}
	// No request can choose a command, hardware path or ownership path.
	directory := "/run/fnos-power-scheduler"
	if err := os.MkdirAll(directory, 0755); err != nil {
		log.Fatal(err)
	}
	info, err := os.Lstat(directory)
	if err != nil || !info.IsDir() || info.Mode()&os.ModeSymlink != 0 {
		log.Fatal("无效的运行目录")
	}
	stat := info.Sys().(*syscall.Stat_t)
	if stat.Uid != 0 || info.Mode().Perm()&0022 != 0 {
		log.Fatal("运行目录必须归 root 所有且不可被其他用户写入")
	}
	ownershipDirectory := "/var/lib/fnos-power-scheduler"
	if err = os.MkdirAll(ownershipDirectory, 0700); err != nil {
		log.Fatal(err)
	}
	ownershipInfo, err := os.Lstat(ownershipDirectory)
	if err != nil || !ownershipInfo.IsDir() || ownershipInfo.Mode()&os.ModeSymlink != 0 {
		log.Fatal("无效的 RTC 所有权目录")
	}
	if ownershipInfo.Sys().(*syscall.Stat_t).Uid != 0 || ownershipInfo.Mode().Perm()&0077 != 0 {
		log.Fatal("RTC 所有权目录必须为 root 专用")
	}
	statePath := filepath.Join(ownershipDirectory, "owned-alarm")
	var owned int64
	if b, e := os.ReadFile(statePath); e == nil {
		owned, err = strconv.ParseInt(strings.TrimSpace(string(b)), 10, 64)
		if err != nil {
			log.Fatal("RTC 所有权记录损坏")
		}
	} else if !os.IsNotExist(e) {
		log.Fatal(e)
	}
	if *cleanup {
		if owned != 0 {
			b, e := os.ReadFile(*rtc)
			if e != nil {
				log.Fatal(e)
			}
			current, e := strconv.ParseInt(strings.TrimSpace(string(b)), 10, 64)
			if strings.TrimSpace(string(b)) == "" {
				current = 0
				e = nil
			}
			if e != nil {
				log.Fatal(e)
			}
			if current == owned {
				if e = os.WriteFile(*rtc, []byte("0"), 0600); e != nil {
					log.Fatal(e)
				}
			}
		}
		if err = os.Remove(statePath); err != nil && !os.IsNotExist(err) {
			log.Fatal(err)
		}
		return
	}
	socket := filepath.Join(directory, "power.sock")
	if _, err = os.Lstat(socket); err == nil {
		log.Fatal("power socket 已存在，请先停止旧进程并清理")
	}
	listener, err := net.Listen("unix", socket)
	if err != nil {
		log.Fatal(err)
	}
	defer os.Remove(socket)
	if err = os.Chown(socket, *uid, -1); err != nil {
		log.Fatal(err)
	}
	if err = os.Chmod(socket, 0600); err != nil {
		log.Fatal(err)
	}
	var mu sync.Mutex
	read := func() (int64, error) {
		b, err := os.ReadFile(*rtc)
		if err != nil {
			return 0, err
		}
		s := strings.TrimSpace(string(b))
		if s == "" {
			return 0, nil
		}
		return strconv.ParseInt(s, 10, 64)
	}
	write := func(n int64) error { return os.WriteFile(*rtc, []byte(strconv.FormatInt(n, 10)), 0600) }
	handler := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		mu.Lock()
		defer mu.Unlock()
		w.Header().Set("Content-Type", "application/json")
		respond := func(code int, s power.Status) { w.WriteHeader(code); json.NewEncoder(w).Encode(s) }
		current, err := read()
		s := power.Status{Mode: "hardware", Available: err == nil, Alarm: current, Message: "检测到 RTC 接口；关机唤醒仍需实机验证"}
		if err != nil {
			s.Message = err.Error()
			respond(503, s)
			return
		}
		if r.Method != "POST" || r.URL.Path != "/control" {
			respond(404, power.Status{Message: "无效请求"})
			return
		}
		var req power.Request
		dec := json.NewDecoder(http.MaxBytesReader(w, r.Body, 1024))
		dec.DisallowUnknownFields()
		if err = dec.Decode(&req); err != nil {
			respond(400, power.Status{Message: err.Error()})
			return
		}
		switch req.Operation {
		case "status":
		case "set", "clear":
			target := req.Alarm
			if req.Operation == "clear" {
				target = 0
			}
			if target != 0 && (target <= time.Now().Unix()+5 || target > time.Now().AddDate(1, 0, 0).Unix()) {
				respond(400, power.Status{Message: "RTC 时间必须在未来 5 秒至一年之间"})
				return
			}
			if current != 0 && current != owned {
				respond(409, power.Status{Message: "RTC 闹钟由其他程序设置，未覆盖"})
				return
			}
			if target != current {
				if err = write(0); err == nil && target != 0 {
					err = write(target)
				}
				if err != nil {
					s.Message = err.Error()
					respond(500, s)
					return
				}
				actual, e := read()
				if e != nil || actual != target {
					s.Message = "RTC 写入后读回不一致"
					respond(500, s)
					return
				}
			}
			temporary, e := os.CreateTemp(ownershipDirectory, "alarm-")
			if e != nil {
				s.Message = e.Error()
				respond(500, s)
				return
			}
			temporaryPath := temporary.Name()
			_, err = temporary.WriteString(strconv.FormatInt(target, 10))
			if err == nil {
				err = temporary.Sync()
			}
			closeErr := temporary.Close()
			if err == nil {
				err = closeErr
			}
			if err == nil {
				err = os.Rename(temporaryPath, statePath)
			}
			os.Remove(temporaryPath)
			if err != nil {
				s.Message = err.Error()
				respond(500, s)
				return
			}
			owned = target
			s.Alarm = target
		case "poweroff":
			if req.Alarm != current || (current != 0 && current != owned) {
				respond(409, power.Status{Message: "关机前 RTC 状态改变，已取消关机"})
				return
			}
			if req.Alarm != 0 && req.Alarm <= time.Now().Unix()+120 {
				respond(409, power.Status{Message: "开机时间距离关机不足两分钟，已取消关机"})
				return
			}
			commandContext, cancel := context.WithTimeout(context.Background(), 5*time.Second)
			defer cancel()
			command := exec.CommandContext(commandContext, "/usr/bin/systemctl", "--no-block", "poweroff")
			if output, e := command.CombinedOutput(); e != nil {
				respond(500, power.Status{Message: fmt.Sprintf("正常关机请求失败：%v %s", e, output)})
				return
			}
			s.Message = "已请求系统正常关机"
		default:
			respond(400, power.Status{Message: "不允许的电源操作"})
			return
		}
		respond(200, s)
	})
	// Restrictive socket mode authenticates the dedicated package user; its parent is root-owned.
	server := &http.Server{Handler: handler, ReadHeaderTimeout: 3 * time.Second, ReadTimeout: 5 * time.Second, WriteTimeout: 10 * time.Second}
	signals := make(chan os.Signal, 1)
	signal.Notify(signals, syscall.SIGTERM, syscall.SIGINT)
	go func() { <-signals; server.Close() }()
	if err = server.Serve(authenticatedListener{Listener: listener, uid: uint32(*uid)}); err != nil && err != http.ErrServerClosed {
		log.Fatal(err)
	}
}
