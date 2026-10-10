# fnos-power-scheduler · 电源计划

面向飞牛 fnOS x86_64 的电源计划应用，当前版本为 **0.2.1 开发版**。使用 Rust、Vue 和 SQLite，通过 systemd timer 调度关机，通过 RTC 硬件闹钟定时开机。

## 功能

- 支持单次、每天、每周计划，每天可安排多组开关机。
- 总览固定展示周一至周日，按时间顺序显示开关机，单次计划按日期另列。
- 提示连续开关机、重复时间、同时间开关机以及关机后五分钟内开机的安排；提示可展开，不阻止保存。重复计划与单次计划分别检查，同一时区内比较。
- 下一次开关机时间包含日期、星期和时间，界面采用紧凑布局。
- 关机前 60 秒倒计时，可取消本次关机；启动保护、任务去重、错过关机不补执行。
- 每项计划保存 IANA 时区；执行记录和计划保存在 SQLite 中。
- 关机前写入并读回 RTC，不覆盖其他程序拥有的闹钟；应用计划失败时显示原因。

**关闭页面或停止应用后，已应用的计划仍会执行。** 停止应用会取消当前关机倒计时；需要暂停时先禁用计划。卸载会清理本应用 timer 和仍归本应用所有的 RTC 闹钟。

RTC 后端支持 UTC 硬件时钟，检测到 `/etc/adjtime` 的 `LOCAL` 配置时拒绝写入。尚未自动检测备份、存储维护及 Docker 工作负载，计划时间需避开繁忙时段。不同设备的 RTC 唤醒能力应分别测试。

## 本地开发

需要 Rust 1.96+、Node.js 22.12+ 和 Python 3.9+。

```powershell
npm --prefix web ci
npm --prefix web run build
cargo test --locked
.\scripts\dev.ps1
```

打开 `http://127.0.0.1:8787`。模拟模式不安装系统 timer，不修改 RTC，不执行关机。调试前端时可另开终端运行 `npm --prefix web run dev`，Vite 会代理后台 API。

## 构建安装包

打包使用 fnpack 1.2.3。Windows 交叉编译还需 Zig 0.14.1。

```powershell
rustup target add x86_64-unknown-linux-musl
.\scripts\build.ps1 -ZigPath C:\tools\zig\zig.exe
```

脚本也会寻找 `work/zig-sdk/ziglang/zig.exe`。安装包为 `dist/fnos-power-scheduler.fpk`，旁边生成 SHA256 校验文件。Linux 构建、运行机制及设备检查命令见 [开发与运行说明](docs/development.md)。

```powershell
python scripts/fpk.py verify dist/fnos-power-scheduler.fpk
python -m unittest discover -s scripts -p test_fpk.py
```

打包校验检查安装包结构、Linux x86_64 程序、脚本换行和执行权限。

## 安装与运行

安装包下载：[GitHub Releases](https://github.com/Simlym/fnos-power-scheduler/releases)。正式版的最新安装包可通过[固定下载链接](https://github.com/Simlym/fnos-power-scheduler/releases/latest/download/fnos-power-scheduler.fpk)获取；预发布版本在 Releases 列表单独展示。每个版本附带 SHA256 校验文件。

通过飞牛应用管理手动安装 FPK。更新前备份应用数据。

网页通过飞牛网关 Unix Socket 提供服务，验证网关对端 UID、管理员身份和操作校验 Header。网页由普通应用用户运行，电源程序位于独立的 root 所有目录，由系统按需启动。

项目处于开发阶段，尚未通过应用中心审核。最低系统版本声明为 `1.1.3100`，具体设备兼容性以实测为准；许可证尚待确定。

## GitHub 自动构建与发布

提交到 `main`、向 `main` 提交 PR 或在 Actions 页面手动运行 `Build and release` 时，会校验版本、运行 Python 和 Rust 测试、构建前端及 Linux x86_64 程序，并验证 FPK。成功的安装包和 SHA256 文件保存在该次运行的 Artifacts 中，保留 14 天。

推送 `v*` 版本标签时，构建成功后自动创建 GitHub Release，并上传安装包和校验文件。仅发布任务拥有 `contents: write` 权限，不需要配置个人访问令牌。构建工具版本固定，fnpack 下载后校验 SHA256，GitHub Actions 使用固定提交并由 Dependabot 检查更新。

发布前同步 `Cargo.toml`、`fpk/manifest`、`web/package.json`、`web/package-lock.json` 的版本（包括 lock 文件根包版本），并更新 README 和开发说明中的版本描述。标签必须与版本完全一致。例如版本为 `0.2.1` 时：

```bash
python scripts/check_version.py --tag v0.2.1
git push origin main
git tag -a v0.2.1 -m "Release 0.2.1"
git push origin v0.2.1
```

版本包含后缀（例如 `0.2.2-rc.1`）时，标签为 `v0.2.2-rc.1`，自动标记为预发布且不更新 latest 下载链接。首次发布前在仓库 Settings → Actions → General 确认允许 GitHub Actions 运行。

失败的构建不会发布。修复后发布新的版本标签；若构建失败且尚未创建 Release，可以在 Actions 页面重新运行失败任务。已有 Release 不会被自动覆盖。手动运行工作流只生成构建产物，不发布 Release。

发布到 GitHub 不代表飞牛应用中心上架。真实关机和 RTC 唤醒仍需按[飞牛验收流程](docs/development.md#飞牛验收)在设备上测试。
