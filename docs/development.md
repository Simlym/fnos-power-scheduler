# 开发与运行说明

后端使用 Rust，界面使用 Vue，SQLite 保存计划、执行记录和执行去重信息。当前版本为 0.2.1 开发版。

## 运行方式

- `server`：普通 fnps 用户运行的网页后台，固定四个阻塞式工作线程；没有后台计划扫描。
- `powerd control`：systemd 接收到管理 Socket 连接时启动，处理一个固定格式的请求后退出。
- `powerd execute ID`：每条关机 timer 到点启动；倒计时期间存在，完成或取消后退出。
- `powerd reconcile`：更新最近一次 RTC 唤醒；在开机规则到点、正常系统关机时执行。
- `fnps-rust-lifecycle.service`：启动时恢复计划，退出时刷新 RTC，oneshot 的 active(exited) 状态不表示有进程常驻。

网页后台按需启动暂不实现。关闭浏览器后只保留 Rust server；systemd 管理监听接口和 timer，没有常驻 powerd。
页面打开期间每两秒查询一次，每次查询会短暂启动一个 control 进程。

## 调度行为

**停止应用只停止管理入口，已应用的关机 timer 和 RTC 闹钟继续有效。**
停止应用取消当前正在倒计时的关机。需要暂停所有自动动作时先在页面禁用全部计划。
卸载时停止执行进程、删除本应用 timer，并只清除仍与所有权记录一致的 RTC 闹钟。

每天可以安排多组开关机。每条关机规则有独立 timer；每条开机规则还有一个用于设备仍运行时更新下一次 RTC 的 timer。
RTC 只写最近一次未来开机时间，启动、保存计划、关机前和开机规则到点时更新。
timer 使用 Persistent=false，执行程序另外校验任务是否仍启用、当前分钟是否匹配、是否已处理。
夏令时不存在的本地分钟跳过；重复分钟按本地时间的 occurrence key 去重。

当前 RTC 后端支持 UTC 硬件时钟。检测到 /etc/adjtime 的 LOCAL 时拒绝写入并报告错误，避免出现时区偏移；尚未实现 LOCAL 兼容。
RTC 不存在、被其他程序占用、读回不一致或下一次唤醒距离实际关机不足两分钟，均取消计划关机。
系统启动及启动应用后的两分钟跳过关机。倒计时内系统时间跳变超过三秒取消本次关机。

## 数据与权限

安装启动时将应用数据目录及 SQLite 文件修复为 root 专用，保留已有计划和记录。
fnps 网页进程通过 /run/fnos-power-scheduler/power.sock 请求数据；root 电源程序再次校验 SO_PEERCRED。
网关身份、管理员 Header 和写操作校验 Header 保持检查；不新增 TCP 管理端口。
root 可执行文件仍复制到 /usr/local/libexec/fnos-power-scheduler，不能从网页可写目录执行。
任务 ID、时区和时间严格校验，生成 service 时不拼接用户任务名称或任意命令。
数据库保存与 timer/RTC 应用分别记录；系统应用失败时页面展示错误，修复后重新保存规则重试。
数据库包含 tasks、records、occurrences、metadata 和 pending 表。

## 构建

Windows：Rust 1.96+、Node.js 22.12+、Python 3.9+、fnpack 1.2.3、Zig 0.14.1。

```powershell
rustup target add x86_64-unknown-linux-musl
cargo test --locked
.\scripts\build.ps1 -ZigPath C:\tools\zig\zig.exe
```

脚本也自动寻找 work/zig-sdk/ziglang/zig.exe。Zig 编译捆绑 SQLite 并静态链接 musl，产物为 Linux x86_64 ELF。
Linux 原生构建需要安装目标对应的 C 编译器，例如 musl-gcc，并设置 CC/linker；不依赖飞牛系统安装 Rust。

```bash
rustup target add x86_64-unknown-linux-musl
CC_x86_64_unknown_linux_musl=musl-gcc \
CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=musl-gcc \
bash scripts/build.sh
```

最终安装包：dist/fnos-power-scheduler.fpk，旁边有 SHA256 校验文件。

本地模拟网页：先构建 web，然后 cargo run --locked --bin server，打开 http://127.0.0.1:8787。
模拟模式只管理规则和显示下一次时间，不安装 timer，不执行倒计时或电源操作。

## 飞牛验收

更新前备份应用数据，通过应用管理界面安装新的 FPK。

```bash
ps -p "$(cat /run/fnos-power-scheduler/server.pid)" -o pid,user,rss,%cpu,args
pgrep -af '[p]owerd'
systemctl list-timers --all 'fnps-rust-plan-*'
systemctl status fnps-rust-control.socket fnps-rust-lifecycle.service
journalctl -u 'fnps-rust-control@*' -u 'fnps-rust-plan-*' --since '10 minutes ago'
cat /sys/class/rtc/rtc0/wakealarm
```

关闭页面后 powerd 应不存在；server 保持运行。页面刷新期间短暂出现 powerd 是正常情况。
用一次关机倒计时取消和一次真实关机/RTC 唤醒测试，再验证两组以上开关机以及重启后计划恢复。
确认停止应用后 timer 仍存在、禁用规则后 timer 删除、卸载后无遗留 timer。
Unix Socket 激活、应用生命周期和真实 RTC 唤醒仍需在目标 fnOS 实测，本地自动测试不能代替实机验收。
