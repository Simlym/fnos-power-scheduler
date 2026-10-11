# fnos-power-scheduler · 电源计划

为飞牛 fnOS 提供定时关机与 RTC 定时开机的应用，支持单次、每天和每周计划，以及一天内多组开关机安排。计划和执行记录保存在本机，关机由 systemd timer 调度，开机由设备的 RTC 硬件闹钟触发。

## 下载与安装

1. 在 [GitHub Releases](https://github.com/Simlym/fnos-power-scheduler/releases) 下载 `fnos-power-scheduler.fpk`，也可使用[最新正式版固定下载链接](https://github.com/Simlym/fnos-power-scheduler/releases/latest/download/fnos-power-scheduler.fpk)。
2. 通过飞牛应用管理的手动安装入口安装 FPK；更新前先备份应用数据。
3. 打开「电源计划」，添加并保存计划，检查应用结果及下一次开关机时间。
4. 首次使用先验证关机倒计时取消，再在设备上测试真实关机与 RTC 开机。

每个发布版本附带 `fnos-power-scheduler.fpk.sha256` 校验文件。请下载 FPK 附件安装；GitHub 自动生成的 Source code 压缩包是源码。

### 适用条件

- 当前安装包面向 **fnOS x86_64**，声明的最低系统版本为 **1.1.3100**。
- 定时开机需要主板和固件支持关机状态下的 RTC 唤醒，不同设备需分别测试。
- 当前支持 UTC 硬件时钟；检测到 `/etc/adjtime` 为 `LOCAL` 时拒绝写入并显示错误。
- 项目仍在开发阶段，尚未通过飞牛应用中心审核；GitHub 发布不等于应用中心上架。

## 功能与使用

- **灵活计划**：单次、每天、每周规则；每项计划独立保存 IANA 时区。
- **计划总览**：按周一至周日和时间顺序展示周期计划，单次计划按日期另列；显示下一次开关机的日期、星期和时间。
- **安排提示**：提示重复时间、连续开关机、同时间开关机及关机后五分钟内开机等情况，提示不阻止保存。周期与单次计划分别检查，同一时区内比较。
- **关机保护**：关机前 60 秒倒计时，可取消本次关机；提供启动保护和任务去重，错过关机不会补执行。
- **执行反馈**：保存计划和执行记录，显示系统应用失败的原因；关机前写入并读回 RTC，不覆盖其他程序拥有的闹钟。

**关闭页面或停止应用后，已应用的计划仍会执行。** 停止应用会取消当前关机倒计时；需要暂停自动开关机时，应先禁用全部计划。卸载会清理本应用 timer，以及仍归本应用所有的 RTC 闹钟。

应用尚未自动检测备份、存储维护及 Docker 工作负载，请将关机时间安排在设备空闲时段。设备检查、调度细节及实机验收见[开发与运行说明](docs/development.md)。

## 本地开发

需要 Rust 1.96+、Node.js 22.12+ 和 Python 3.11+（版本检查使用标准库 `tomllib`）。后端使用 Rust，前端使用 Vue，数据使用 SQLite。

在项目根目录执行，Windows、Linux 和 macOS 均可用于本地模拟开发：

```sh
npm --prefix web ci
npm --prefix web run build
cargo test --locked
python scripts/check_version.py
python -m unittest discover -s scripts -p 'test_*.py'
cargo run --locked --bin server
```

若系统的 Python 命令为 `python3`，请相应替换。打开 `http://127.0.0.1:8787`；模拟模式不安装系统 timer、不修改 RTC、不执行关机。调试前端时可另开终端运行 `npm --prefix web run dev`，Vite 会代理后台 API。

## 构建飞牛安装包

构建目标为 Linux x86_64，打包使用 fnpack 1.2.3。设备本身不需要安装开发工具。

### Linux

准备上述开发工具、`musl-gcc` 和 fnpack，将 fnpack 放入 PATH，然后执行：

```sh
rustup target add x86_64-unknown-linux-musl
CC_x86_64_unknown_linux_musl=musl-gcc \
CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=musl-gcc \
bash scripts/build.sh
```

### Windows

另外准备 Zig 0.14.1 和 Windows 版 fnpack，将工具路径替换为本机位置：

```powershell
rustup target add x86_64-unknown-linux-musl
.\scripts\build.ps1 -ZigPath '路径\zig.exe' -FnpackPath '路径\fnpack.exe'
```

两种方式都会生成 `dist/fnos-power-scheduler.fpk` 和同名 `.sha256` 文件。可进一步检查安装包结构、Linux x86_64 程序、脚本换行和执行权限：

```sh
python scripts/fpk.py verify dist/fnos-power-scheduler.fpk
```

## GitHub 自动构建与发布

| 触发方式 | 构建结果 | 是否发布 Release |
| --- | --- | --- |
| 提交到 `main`、向 `main` 提交 PR | 测试、构建、验证 FPK；产物保存在 Actions 的 Artifacts 中，保留 14 天 | 否 |
| 在 Actions 手动运行 `Build and release` | 同上 | 否 |
| 推送与项目版本一致的 `v*` 标签 | 同上；自动创建 Release 并上传 FPK 和 SHA256 文件 | 是 |

**无需手工把安装包上传到标签。** 推送版本标签后，等待 `Build and release` 成功，安装包会自动出现在对应 Release 的 Assets 下载区。发布使用 GitHub 提供的令牌，无需另配个人访问令牌。

发布前同步 `Cargo.toml`、`Cargo.lock` 中的项目版本、`fpk/manifest`、`web/package.json` 和 `web/package-lock.json`（包括根包版本）。先提交版本变更，再发布与版本完全一致的标签。例如下一版本为 `0.2.3` 时：

```sh
python scripts/check_version.py --tag v0.2.3
git push origin main
git tag -a v0.2.3 -m "Release 0.2.3"
git push origin v0.2.3
```

带后缀的版本（如 `0.2.3-rc.1`）会标记为预发布，不更新最新正式版固定下载链接。构建失败不会发布；修复后发布新版本标签，或在尚未创建 Release 时重新运行失败任务。已有 Release 不会被自动覆盖。

正式发布前需确认仓库允许 GitHub Actions 运行。自动构建不能代替目标设备上的真实关机及 RTC 唤醒测试。

## 更多说明

- [开发、权限模型、调度机制与设备验收](docs/development.md)
- [版本下载与更新记录](https://github.com/Simlym/fnos-power-scheduler/releases)
- [问题反馈](https://github.com/Simlym/fnos-power-scheduler/issues)

许可证尚待确定。
