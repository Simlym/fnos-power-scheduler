# fnos-power-scheduler · 电源计划

面向飞牛 fnOS x86_64 的原生电源计划应用：定时关机、RTC 定时开机、可取消倒计时和执行记录。

当前是 **0.1.0 开发版**。已经实现本地模拟运行及 Linux 构建；飞牛安装、网关进程身份、RTC 和真实关机仍需实机验收。不是已经通过应用中心审核的版本。

## 功能

- 单次、每天、每周指定日期的开机和关机计划。
- 每条规则保存 IANA 时区，展示下一次执行时间。
- 关机前 60 秒倒计时，可取消本次执行。
- 启动后两分钟保护；错过的关机不补执行。
- SQLite 保存计划、执行记录与已处理的任务时间，防止同一任务重复触发。
- 拒绝同一时刻的开关机规则冲突；关机前再次写入并核对下一次 RTC 闹钟。
- 不覆盖其他程序设置的 RTC 闹钟。RTC 设置失败时保留任务并显示错误，取消到期关机。

## 本地开发（Windows）

需要 Go 1.24+ 和 Node.js 22.12+。打包还需要 Python 3.9+ 和官方 fnpack 1.2.3（用 `fnpack --help` 查看版本）。开发脚本优先使用忽略目录 `work/sdk/go` 中的 Go，再寻找系统 Go。

```powershell
cd web
npm ci
npm run build
cd ..
.\scripts\dev.ps1
```

打开 `http://127.0.0.1:8787`。默认使用模拟电源控制，不会关机或修改 RTC。需要前端热更新时，在另一个终端运行 `cd web; npm run dev`，使用它打印的本机地址。

## 测试与构建

```powershell
go test ./...
.\scripts\build.ps1
```

Windows 构建默认使用 `work/fnpack.exe` 或 PATH 中的 `fnpack`，也可以指定路径：

```powershell
.\scripts\build.ps1 -FnpackPath C:\tools\fnpack.exe
```

Linux / WSL 构建：

```bash
bash scripts/build.sh
# 工具不在 PATH 时：
GO_BIN=/path/to/go FNPACK_BIN=/path/to/fnpack bash scripts/build.sh
```

真正可安装的产物是 `dist/fnos-power-scheduler.fpk`，不是 `dist/fpk` 暂存目录。脚本缺少打包工具或校验失败时返回错误；每次重建清理暂存目录和旧安装包。

fnpack 生成后，`scripts/fpk.py` 将内外两层归档统一为 USTAR，目录为 0755、普通文件为 0644、生命周期脚本和两个程序为 0755，归档所有者为 root。这样 Windows 源文件缺少 Unix 执行位时，也不依赖安装回调先执行才能修复权限。修改 `app.tgz` 后同步更新 manifest 中的 MD5 checksum，并检查包结构、脚本换行、JSON 配置、Linux x86_64 ELF 程序和权限；旁边生成 SHA256 文件用于核对传输。

已有安装包可以独立检查：

```bash
python3 scripts/fpk.py verify dist/fnos-power-scheduler.fpk
```

归档检查通过不代表已通过飞牛实机安装验收。如报“应用包格式不符合系统版本要求”，请提供实际上传的 FPK 和 NAS 的完整 fnOS 系统版本（NAS 网页设置中查看，手机 App 的版本不等于系统版本）。当前保留 `os_min_version=1.1.3100`，不通过降低版本声明绕过网关兼容性要求。

图标源生成脚本：`scripts/generate-icons.ps1`。GitHub 仓库已经包括生成的 PNG，无需每次构建重新生成。

## 系统集成

网页服务使用普通包用户 `fnps` 运行，只监听飞牛网关 Unix Socket，不开放管理 TCP 端口。

飞牛网关入口为 `/app/fnos-power-scheduler/`。页面与 API 都要求网关提供 `X-Trim-Userid` 和 `X-Trim-Isadmin=true`。服务同时验证 Unix Socket 对端 UID，不能仅伪造 Header 绕过权限。

网关 UID 默认设为 `0`，**必须在目标飞牛系统核实**。若网关不是 root，由管理员将真实数字 UID 写入应用配置目录的 `gateway-uid`，再启动。不能为了绕过问题禁用对端校验。

电源辅助进程以 root 运行，其 Socket 位于 root 拥有的 `/run/fnos-power-scheduler/power.sock`。Socket 仅允许应用用户访问，并再次验证对端 UID。只提供 RTC 查询、设置、清除及正常关机四种固定操作，不提供任意命令执行。

安装与升级回调将电源程序复制到 root 拥有的 `/usr/local/libexec/fnos-power-scheduler/powerd`，并设置 Linux 可执行权限。网页服务可写的应用目录不会作为 root 电源程序的运行路径。

RTC 所有权记录位于 `/var/lib/fnos-power-scheduler/owned-alarm`，仅 root 可写，跨重启保留。RTC 默认使用 `/sys/class/rtc/rtc0/wakealarm`。安装包当前固定使用 rtc0；多 RTC 设备需在实机探测后调整。

正常关机使用 `/usr/bin/systemctl --no-block poweroff`，不强制断电。前端“已请求”表示请求已提交，不代表已经验证整机断电。

### 停止与卸载

停止应用会暂停后台关机调度，但**保留已写入的 RTC 闹钟**。飞牛可能在系统关机时执行应用停止脚本；清除闹钟会破坏下一次定时开机。

需要取消开机时，请在停止应用前禁用或删除开机规则。卸载脚本只清除仍与本应用所有权记录一致的闹钟，保留其他程序后来设置的闹钟。

## 飞牛实机验收

1. 安装开发包，确认普通用户无法访问页面和 API，管理员可以打开。
2. 确认进程 UID、网关 UID、Socket 权限及系统关机程序路径。
3. 创建未来的开机任务，确认 RTC 读回时间正确；与 BIOS 的定时设置不要同时维护同一个闹钟。
4. 创建关机任务，先验证倒计时取消，再验证一次正常关机和后续 RTC 唤醒。开机时间与关机至少间隔数分钟；辅助进程拒绝距离开机不足两分钟的关机请求。
5. 验证重启、停止/启动、时区变化和修改规则后不会补关机；断电恢复后重新计算计划。
6. 验证禁用全部开机规则、其他程序修改闹钟、RTC 写入失败及卸载清理。

第一版没有自动检测飞牛备份、存储维护或 Docker 工作负载是否繁忙；安排关机时需自行避开这些任务。固定时区规则不会自动跟随系统时区变更，应通过编辑任务调整。

最低系统版本暂按官方统一网关示例使用 `1.1.3100`，并不代表已完成该版本实测。正式发布前应依据实测填写。发布者与维护者信息也需要替换为真实信息。

## 官方参考

- [应用权限](https://developer.fnnas.com/docs/core-concepts/privilege/)
- [统一网关](https://developer.fnnas.com/docs/core-concepts/gateway-registration/)
- [Manifest](https://developer.fnnas.com/docs/core-concepts/manifest/)
- [fnpack](https://developer.fnnas.com/docs/cli/fnpack/)
- [上架应用](https://developer.fnnas.com/docs/quick-started/publish-application/)

许可证暂未确定；发布开源仓库前请选定许可证。
