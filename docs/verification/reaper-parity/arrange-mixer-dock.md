# SHELL-DOCK-001 — Arrange 与 Mixer 同窗布局

2026-10-14，Linux REAPER 7.82 / Default 7。此项原先在矩阵中标成 `uninvestigated`，并称 Arrange 与 Mixer 互斥；源码核查和实窗复验表明，该描述已过时。Linux Desktop profile 默认在同一主窗显示 Arrange 与 Mixer。互斥的 Arrange/Mixer 工作区切换仅用于 Touch profile。

原厂参照使用新建的隔离 REAPER 配置，Default 主题保持默认；虚拟环境没有音频硬件，因此仅把音频设备设为 REAPER 自带的 Dummy Audio。冷启动空工程截图显示 Arrange、Transport 与 Mixer 同时可见，Master 位于 Mixer 左侧。随后用原厂 Ctrl+T 插入一条空轨，传输保持停止；同一轨道同时出现在 TCP 和 MCP。没有媒体、录音或音频播放。逐张图的状态和 SHA-256 也保存在 [结构化 fixture](fixtures/arrange-mixer-dock.tsv)。

AAADAW 实窗截图中 Arrange 与 Mixer 同时显示，Audio 1 同时出现在轨道控制区和 Mixer；工程没有媒体，Transport 显示停止，当前构建没有音频设备。它和 REAPER 截图窗口尺寸不同，因此这组图只核对同窗共存与轨道身份，不作为像素尺寸、默认分割比例或主题视觉通过证据。

| 证据 | SHA-256 | 观察 |
| --- | --- | --- |
| [REAPER 冷启动默认布局](fixtures/arrange-mixer-native-default.png) | `f7db67cf9a3897af5c4d97fc3ee59b7c5758f2ec46aacf95aa04ea2196568cb3` | 新配置空工程；Arrange、Transport、Mixer 同时可见 |
| [REAPER 加入空轨](fixtures/arrange-mixer-native.png) | `803db3e7f313b624b158d7c3976880a711c7059cb7e1ffe6fbe666a2958d3208` | Track 1 同时出现在 TCP/MCP；传输停止 |
| [AAADAW 实窗](fixtures/arrange-mixer-aaadaw.png) | `6510de2b0235686aa1e48b02f370f9e6ac0886e1fb2ef4350fbe378864c1391e` | Audio 1 同时出现在 Arrangement/Mixer；传输停止 |

源码中，`DesktopLayout::default` 默认启用 Mixer；`MediaPanelDock::rebuild` 将 Mixer 分割到 Arrangement 下方；桌面视图从同一个 App/Project 为两个 pane 构建内容。`desktop_mixer_dock_and_media_share_state_and_keep_independent_ratios` 覆盖显示/隐藏 Mixer、Media Browser 和 Mixer 独立比例以及切换前后轨道 ID 不变；`desktop_layout_round_trip_and_invalid_values_preserve_existing_file` 覆盖布局持久化和非法文件保留。

这还不是 Docker parity 通过：REAPER 的 Docker 可停靠到不同边缘、使用标签页或浮动；AAADAW 目前只提供有限的 pane grid。默认比例、分隔线拖动边界、最小尺寸、完整缩放恢复、多屏/DPI，以及 TCP 与 MCP 两侧修改控件后的逐项双向同步尚未按原厂做完整 GUI 验收。矩阵因此保持 `in_progress`。
