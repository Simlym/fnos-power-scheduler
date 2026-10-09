package httpapi

import (
	"context"
	"encoding/json"
	"fnos-power-scheduler/internal/app"
	"fnos-power-scheduler/internal/peer"
	"fnos-power-scheduler/internal/schedule"
	"net"
	"net/http"
	"strconv"
	"strings"
)

type peerKey struct{}

func PeerContext(ctx context.Context, c net.Conn) context.Context {
	uid, err := peer.UID(c)
	if err != nil {
		return ctx
	}
	return context.WithValue(ctx, peerKey{}, uid)
}
func Handler(a *app.App, directory string, hardware bool, gatewayUID uint32) http.Handler {
	mux := http.NewServeMux()
	respond := func(w http.ResponseWriter, v any, status int) {
		w.Header().Set("Content-Type", "application/json; charset=utf-8")
		w.Header().Set("Cache-Control", "no-store")
		w.WriteHeader(status)
		json.NewEncoder(w).Encode(v)
	}
	fail := func(w http.ResponseWriter, err error, status int) {
		respond(w, map[string]string{"error": err.Error()}, status)
	}
	mux.HandleFunc("GET /api/state", func(w http.ResponseWriter, r *http.Request) {
		v, err := a.View()
		if err != nil {
			fail(w, err, 500)
			return
		}
		respond(w, v, 200)
	})
	mux.HandleFunc("POST /api/tasks", func(w http.ResponseWriter, r *http.Request) {
		var t schedule.Task
		decoder := json.NewDecoder(http.MaxBytesReader(w, r.Body, 8192))
		decoder.DisallowUnknownFields()
		if err := decoder.Decode(&t); err != nil {
			fail(w, err, 400)
			return
		}
		if err := a.Save(t); err != nil {
			fail(w, err, 400)
			return
		}
		respond(w, map[string]bool{"ok": true}, 200)
	})
	mux.HandleFunc("DELETE /api/tasks/{id}", func(w http.ResponseWriter, r *http.Request) {
		if err := a.Delete(r.PathValue("id")); err != nil {
			fail(w, err, 400)
			return
		}
		respond(w, map[string]bool{"ok": true}, 200)
	})
	mux.HandleFunc("POST /api/cancel", func(w http.ResponseWriter, r *http.Request) {
		if err := a.Cancel(); err != nil {
			fail(w, err, 400)
			return
		}
		respond(w, map[string]bool{"ok": true}, 200)
	})
	mux.Handle("/", http.FileServer(http.Dir(directory)))
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("X-Content-Type-Options", "nosniff")
		if hardware {
			uid, ok := r.Context().Value(peerKey{}).(uint32)
			_, userErr := strconv.ParseUint(r.Header.Get("X-Trim-Userid"), 10, 32)
			if !ok || uid != gatewayUID || userErr != nil || r.Header.Get("X-Trim-Isadmin") != "true" {
				http.Error(w, "仅允许通过飞牛网关登录的管理员访问", 403)
				return
			}
		} else {
			host, _, err := net.SplitHostPort(r.Host)
			if err != nil {
				host = r.Host
			}
			if host != "127.0.0.1" && host != "localhost" && host != "[::1]" {
				http.Error(w, "开发模式仅接受本机访问", 403)
				return
			}
		}
		// Custom header prevents cross-origin forms and fetches without a successful CORS preflight.
		if r.Method != "GET" && r.Method != "HEAD" && r.Header.Get("X-Power-Scheduler") != "1" {
			http.Error(w, "缺少操作校验头", 403)
			return
		}
		if hardware {
			const prefix = "/app/fnos-power-scheduler"
			if r.URL.Path == prefix {
				http.Redirect(w, r, prefix+"/", http.StatusTemporaryRedirect)
				return
			}
			if !strings.HasPrefix(r.URL.Path, prefix+"/") {
				http.NotFound(w, r)
				return
			}
			r.URL.Path = strings.TrimPrefix(r.URL.Path, prefix)
		}
		mux.ServeHTTP(w, r)
	})
}
