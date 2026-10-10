# fnos-power-scheduler · 电源计划

面向飞牛 fnOS x86_64 的电源计划应用，当前版本为 **Rust 0.2.1 开发版**。保留 Vue 网页界面，使用 systemd timer 调度关机、RTC 硬件闹钟定时开机。真实安装、系统调度和唤醒兼容性仍需在目标设备验收。

## 功能与运行方式

- 支持单次、每天、每周计划，每天可以设置多组开关机。
- 每条规则保存 IANA 时区，显示下一次执行时间。
- 关机前 60 秒倒计时，可以取消本次执行。
- 启动保护、任务去重和错过关机不补执行。
- 保留旧 SQLite 计划和执行记录；系统计划应用失败时显示错误。
- 关机前写入并读回 RTC；不覆盖其他程序拥有的闹钟。
- Rust 网页后台常驻，不每秒扫描计划；电源管理及关机程序由系统按需启动。

**关闭页面或停止应用后，已应用的计划仍然有效。** 停止应用会取消当前关机倒计时；需要暂停自动动作时，先禁用计划。卸载会清理本应用 timer 和仍归本应用所有的 RTC 闹钟。

RTC 后端目前只支持 UTC 硬件时钟，检测到 `/etc/adjtime` 的 `LOCAL` 配置时拒绝写入。尚不自动检测备份、存储维护和 Docker 工作负载，请自行避开繁忙时间。

## 开发与构建

需要 Rust 1.96+、Node.js 22.12+、Python 3.9+。FPK 打包使用 fnpack 1.2.3。Windows 交叉编译另需 Zig 0.14.1。

```powershell
npm --prefix web ci
npm --prefix web run build
cargo test --locked
.\scripts\dev.ps1
```

打开 `http://127.0.0.1:8787`。本地模拟模式不安装系统 timer，不修改 RTC，不执行关机。

Windows 打包：

```powershell
rustup target add x86_64-unknown-linux-musl
.\scripts\build.ps1 -ZigPath C:\tools\zig\zig.exe
```

脚本也会寻找 `work/zig-sdk/ziglang/zig.exe`。Linux 原生构建方式见 [Rust 实现与迁移说明](docs/rust-migration.md)。

可安装产物为 `dist/fnos-power-scheduler.fpk`，旁边生成 SHA256 校验文件。FPK 结构、Linux x86_64 程序、脚本换行和执行权限均由打包校验器检查。

```powershell
python scripts/fpk.py verify dist/fnos-power-scheduler.fpk
python -m unittest discover -s scripts -p test_fpk.py
```

## 部署与验收

网页只监听飞牛网关 Unix Socket，并验证网关对端 UID、登录管理员信息及操作校验 Header。网页使用普通应用用户运行，root 电源程序位于独立的 root 所有目录。

[迁移说明](docs/rust-migration.md)包含进程、timer、日志和 RTC 的验证命令，以及升级、卸载和回退步骤。升级前备份应用数据；回退旧版前应禁用计划并卸载 Rust 版，避免遗留 timer 与旧调度器同时执行。

旧 Go 代码暂保留作迁移参考，不参与当前构建或安装包；历史文档见 [Go 0.1.4 说明](docs/go-0.1.4.md)。

应用尚未通过应用中心审核。最低版本声明 `1.1.3100` 不等于已完成该版本实测；许可证尚待确定。
