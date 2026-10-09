//go:build !linux

package main

import "log"

func main() { log.Fatal("真实电源控制仅支持 Linux；本机开发请运行模拟服务器") }
