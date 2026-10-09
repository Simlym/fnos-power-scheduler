//go:build linux

package peer

import (
	"net"
	"syscall"
)

func UID(conn net.Conn) (uint32, error) {
	raw, err := conn.(*net.UnixConn).SyscallConn()
	if err != nil {
		return 0, err
	}
	var uid uint32
	var inner error
	err = raw.Control(func(fd uintptr) {
		var credentials *syscall.Ucred
		credentials, inner = syscall.GetsockoptUcred(int(fd), syscall.SOL_SOCKET, syscall.SO_PEERCRED)
		if inner == nil {
			uid = credentials.Uid
		}
	})
	if err != nil {
		return 0, err
	}
	return uid, inner
}
