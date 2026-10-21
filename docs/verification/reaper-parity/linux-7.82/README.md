# LNX-782-ACTIONS-001 — 默认动作目录

2026-10-09 在 Debian 13 x86_64 / X11 Xvfb 中使用新建隔离配置运行 REAPER `7.82/linux-x86_64`，主题为 `Default_7.0.ReaperThemeZip`。来源、安装包与导出文件的 SHA-256、环境和限制见 [manifest.json](manifest.json)。没有加载个人配置、SWS 或第三方动作。

[actions.tsv](actions.tsv) 保存 Section ID、Action ID、英文名称和显式快捷键描述；[sections.tsv](sections.tsv) 保存各 Section 的计数。六个 Section 共 10,640 个条目、488 个显式绑定；包含 Main 与 Main (alt recording) 的重复动作，不能据此计算功能数量或完成率。alt recording 没有显式绑定，不意味着其运行期间完全没有快捷键。

| Section ID | 动作条目 | 显式绑定 |
| --- | ---: | ---: |
| 0 Main | 4379 | 249 |
| 100 Main (alt recording) | 4379 | 0 |
| 32060 MIDI editor | 1103 | 119 |
| 32061 MIDI event list | 299 | 37 |
| 32062 MIDI inline editor | 187 | 64 |
| 32063 Media Explorer | 293 | 19 |

实际导出结果显示，Main 的 `40044 Transport: Play/stop` 是 Space，`40073 Transport: Play/pause` 是 Enter / Ctrl+Space，`40001 Track: Insert new track` 是 Ctrl+T，`40078 View: Toggle mixer visible` 是 Ctrl+M，`1157 Options: Toggle snapping` 是 Alt+S，`40605 Show action list` 是 ?。单独 Play、Pause、Stop 动作没有显式绑定。AAADAW 尚未据此整体替换默认映射；先实现准确的动作语义，再迁移映射，避免把现有行为绑定到不等价的动作上。

复现时从 [官方固定版本地址](https://www.reaper.fm/files/7.x/reaper782_linux_x86_64.tar.xz) 下载并校验归档，使用新目录和独立图形会话，创建输出目录后执行：

```bash
AAADAW_REAPER_REFERENCE_OUTPUT=/absolute/path/to/capture \
  /absolute/path/to/REAPER/reaper \
  -cfgfile /absolute/path/to/fresh-config/reaper.ini -newinst -nosplash \
  /absolute/path/to/AAADAW/scripts/reaper_parity/export-reference.lua
```

首次启动需处理评估提示和音频设备提示，核对默认主题。脚本只读取动作/绑定，不修改项目和偏好，不注册自定义动作，也不请求退出。输出 `runtime.txt` 用于核对版本、资源路径和主题；每次采集另建 manifest，不覆盖本次证据。

本目录没有 REAPER 二进制、主题资产或截图。此证据不覆盖菜单状态、鼠标手势、Preferences 全树、音频设备、DPI/多屏或性能验收；P0 仍在进行。
