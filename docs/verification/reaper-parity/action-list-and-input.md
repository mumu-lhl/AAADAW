# AAADAW-INPUT-002 — 动作列表与数字键盘

2026-10-09，在唯一分支 `feat/reaper-parity` 继续实现 `UI-030` / `INPUT-BIND-001`，并推进 `SHELL-PROFILE-001`。

## 实际参照：LNX-782-ACTIONLIST-002

仍为隔离 Linux REAPER 7.82 / Default 7，X11 Xvfb 1920×1080、无窗口管理器。Action List 客户端为 701×475；绑定输入窗口为 387×229。以下为运行观察，不是完整 P0 验收：

- 顶部 Filter / Clear / Options / Find shortcut / Section，列表列为 Shortcut / Description / State。
- 底部有选中动作的快捷键列表、Add / Delete、Key map / Menu editor、自定义动作管理和 Run / Run-close / Close。选中空历史中的 Undo 时 Run 和 Run/close 仍启用。
- Add 打开 Keyboard/MIDI/OSC Input；默认录入后仍需 OK，自动关闭选项默认未选。包含特殊键入口、CC 参数及 Scope。
- 临时给 Undo 添加 Ctrl+Alt+7 后，原 Show undo history 动作和 Undo 在列表中都保留这个组合；没有出现拒绝冲突提示。关闭列表、新增一条轨道后按此组合执行 Undo，轨道消失，没有打开历史窗口。该观察证明目前的跨动作拒绝策略与参照不同；未推断所有重复绑定的优先级规则。
- 空工程默认 Mixer 可见，Arrange 在上方、Transport 位于 Arrange 和 Mixer 之间，Master strip 位于 Mixer 左端。

内部参照截图保留在采集环境的 `/tmp/aaadaw-reaper-782/`，不提交为产品资源。绑定实验只修改临时隔离会话；动作目录 `LNX-782-ACTIONS-001` 保留原始默认导出，不用实验配置重写。

## 本批实现

- 主窗口 `?`（US 布局 Shift+/）打开独立 701×475 Actions 窗口。已有动作与宏使用稳定 registry ID，列表支持多词搜索、Toggle 状态、选择、Run / Run-close、快捷键查找、追加与单独删除。绑定保存先完成原子磁盘写入，再发布运行配置，失败保留原绑定及其他 Settings 草稿。
- `ShortcutInput` 贯穿捕获与执行，保留逻辑键、物理键、位置和修饰键。新增 NumPad0–9、运算符、Decimal、Comma、Equal、Enter；NumLock 关闭时按物理身份匹配，NumPad5 的逻辑 Unidentified 也不丢弃。
- 新绑定的 `Standard7` / `NumPad7` 等可区分主键区与数字键盘；旧 `7` / `Enter` / `Home` 保留原来的任意位置逻辑匹配，冲突检查包含其实际重叠。配置仍用文本格式，无工程 schema 迁移。
- F10 菜单启动限定未修饰按键；已配置的裸 F10 优先执行绑定，捕获先于菜单处理。Ctrl/Alt+F10 可执行；各入口保留忙碌和待处理保存确认守卫。
- 桌面和触控由明确 `ShellProfile` 区分；窄 Linux 窗口不再自动进入触控工作流，Android 默认 Touch。
- Project 提供只读 `can_undo` / `can_redo`，菜单可用性反映实际历史；同一领域历史仍由 Project 管理。

## 验证证据

自动回归覆盖：主键区/数字键盘独立分发、NumLock 两种状态、NumPad5 Unidentified、Enter/数字键盘 Enter、运算符、旧配置重叠、F10 修饰组合及捕获、配置保存失败原子性、忙碌辅助窗口输入、保存确认期间不执行工程动作、Run-close 的空历史关闭行为、窄桌面/Android profile、历史可用性。

GUI 使用独立 `/tmp/aaadaw-p1-smoke/` XDG 配置：`?` 打开列表，筛选 Undo，追加 Alt+F12，Find shortcut 找到 Undo，再单独删除 Alt+F12，保留 Ctrl+Z。配置文件相应从 `Mod+Z; Alt+F12` 变为 `Mod+Z`。截图：[追加](action-list-added.png)、[查找](action-list-find.png)、[单独删除](action-list-deleted.png)。本证据不作为主题/像素验收。

## 未完成项

Main 以外的 Section、完整默认动作与功能、动作导入导出、菜单编辑器、原厂自定义动作窗口、特殊键/Scope/MIDI/OSC 输入、录入确认和重复绑定优先级仍未对齐。当前 Add 自动提交且冲突拒绝，与上述参照有已知差异。主题和几何尚未完成像素比较，矩阵保持 `in_progress`。

## CI 基础设施修复

先前 Android 模拟器运行 `37953099817` 的日志显示 `-accel off`、无 `/dev/kvm` 权限、系统级 ANR 和 CPU 压力；AAADAW 进程存活且截图检查已通过。工作流新增 KVM 权限配置与加速预检，显式启用硬件加速，保留原有前台与渲染断言；运行结果需在后续唯一 PR 的 CI 验证。
