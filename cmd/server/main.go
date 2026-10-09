package main

import (
	"context"
	"flag"
	"fnos-power-scheduler/internal/app"
	"fnos-power-scheduler/internal/httpapi"
	"fnos-power-scheduler/internal/power"
	"log"
	"net"
	"net/http"
	"os"
	"os/signal"
	"path/filepath"
	"syscall"
	"time"
)

func main() {
	data := flag.String("data", "work/data", "数据目录")
	web := flag.String("web", "web/dist", "前端目录")
	socket := flag.String("socket", "", "飞牛网关 Unix Socket")
	powerSocket := flag.String("power-socket", "", "特权进程 Unix Socket；未提供时为模拟模式")
	gatewayUID := flag.Uint("gateway-uid", 0, "可信飞牛网关进程 UID")
	flag.Parse()
	if (*socket == "") != (*powerSocket == "") {
		log.Fatal("真实模式必须同时指定 socket 和 power-socket")
	}
	if err := os.MkdirAll(*data, 0700); err != nil {
		log.Fatal(err)
	}
	var controller power.Controller = &power.Mock{}
	hardware := *powerSocket != ""
	if hardware {
		controller = power.Client{Socket: *powerSocket}
	}
	a, err := app.Open(filepath.Join(*data, "scheduler.db"), controller)
	if err != nil {
		log.Fatal(err)
	}
	defer a.Close()
	var listener net.Listener
	if hardware {
		if _, err = os.Lstat(*socket); err == nil {
			log.Fatal("网关 socket 已存在，请先停止旧进程并清理")
		}
		listener, err = net.Listen("unix", *socket)
		if err == nil {
			err = os.Chmod(*socket, 0666)
		}
		defer os.Remove(*socket)
	} else {
		listener, err = net.Listen("tcp", "127.0.0.1:8787")
	}
	if err != nil {
		log.Fatal(err)
	}
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	server := &http.Server{Handler: httpapi.Handler(a, *web, hardware, uint32(*gatewayUID)), ReadHeaderTimeout: 5 * time.Second, ReadTimeout: 15 * time.Second, WriteTimeout: 15 * time.Second, IdleTimeout: 60 * time.Second}
	if hardware {
		server.ConnContext = httpapi.PeerContext
	}
	done := make(chan struct{})
	go func() {
		defer close(done)
		timer := time.NewTicker(time.Second)
		defer timer.Stop()
		a.Tick(time.Now())
		for {
			select {
			case <-ctx.Done():
				return
			case now := <-timer.C:
				a.Tick(now)
			}
		}
	}()
	go func() {
		<-ctx.Done()
		shutdown, cancel := context.WithTimeout(context.Background(), 20*time.Second)
		defer cancel()
		server.Shutdown(shutdown)
	}()
	log.Printf("电源计划启动：%s，真实电源模式=%v", listener.Addr(), hardware)
	if err = server.Serve(listener); err != nil && err != http.ErrServerClosed {
		log.Fatal(err)
	}
	stop()
	<-done
}
