package power

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"net/http"
	"time"
)

type Status struct {
	Mode      string `json:"mode"`
	Available bool   `json:"available"`
	Alarm     int64  `json:"alarm"`
	Message   string `json:"message"`
}
type Request struct {
	Operation string `json:"operation"`
	Alarm     int64  `json:"alarm"`
}
type Controller interface{ Do(Request) (Status, error) }
type Mock struct{ Alarm int64 }

func (m *Mock) Do(r Request) (Status, error) {
	switch r.Operation {
	case "set":
		m.Alarm = r.Alarm
	case "clear":
		m.Alarm = 0
	case "poweroff", "status":
	default:
		return Status{}, errors.New("unknown operation")
	}
	return Status{Mode: "simulation", Available: true, Alarm: m.Alarm, Message: "模拟模式：不会关机，也不会修改硬件闹钟"}, nil
}

type Client struct{ Socket string }

func (c Client) Do(r Request) (Status, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 8*time.Second)
	defer cancel()
	tr := &http.Transport{DialContext: func(ctx context.Context, _, _ string) (net.Conn, error) {
		return (&net.Dialer{}).DialContext(ctx, "unix", c.Socket)
	}}
	defer tr.CloseIdleConnections()
	data, _ := json.Marshal(r)
	req, _ := http.NewRequestWithContext(ctx, "POST", "http://power/control", bytes.NewReader(data))
	response, err := (&http.Client{Transport: tr}).Do(req)
	if err != nil {
		return Status{}, err
	}
	defer response.Body.Close()
	var s Status
	if err = json.NewDecoder(response.Body).Decode(&s); err != nil {
		return s, err
	}
	if response.StatusCode != 200 {
		return s, fmt.Errorf("%s", s.Message)
	}
	return s, nil
}
