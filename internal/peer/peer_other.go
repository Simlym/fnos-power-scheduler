//go:build !linux

package peer

import (
	"errors"
	"net"
)

func UID(net.Conn) (uint32, error) { return 0, errors.New("Unix peer credentials require Linux") }
