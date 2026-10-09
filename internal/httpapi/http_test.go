package httpapi

import (
	"fnos-power-scheduler/internal/app"
	"fnos-power-scheduler/internal/power"
	"net/http/httptest"
	"path/filepath"
	"testing"
)

func TestForgedGatewayHeadersOnTCPAreRejected(t *testing.T) {
	a, err := app.Open(filepath.Join(t.TempDir(), "api.db"), &power.Mock{})
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	h := Handler(a, t.TempDir(), true, 0)
	r := httptest.NewRequest("GET", "http://localhost/app/fnos-power-scheduler/api/state", nil)
	r.Header.Set("X-Trim-Userid", "1000")
	r.Header.Set("X-Trim-Isadmin", "true")
	w := httptest.NewRecorder()
	h.ServeHTTP(w, r)
	if w.Code != 403 {
		t.Fatalf("forged headers accepted: %d", w.Code)
	}
}
func TestMutationRequiresCSRFHeader(t *testing.T) {
	a, err := app.Open(filepath.Join(t.TempDir(), "api.db"), &power.Mock{})
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	h := Handler(a, t.TempDir(), false, 0)
	r := httptest.NewRequest("POST", "http://localhost/api/cancel", nil)
	w := httptest.NewRecorder()
	h.ServeHTTP(w, r)
	if w.Code != 403 {
		t.Fatalf("mutation accepted: %d", w.Code)
	}
}
