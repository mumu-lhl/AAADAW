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

## 2026-10-13 重复快捷键继续推进

根据上面的隔离 REAPER 实测，普通动作之间现在允许保存相同快捷键；Action List 与 Settings 均保留两条绑定。回归确认 Undo 显式绑定到 `Mod+S` 时仍保留 Save project 的工厂默认 `Mod+S`，运行时由显式动作映射优先处理，并确认磁盘写入失败仍不改变运行中的绑定。不同普通动作均显式绑定同一组合时，配置验证也保留两条记录。

宏仍单独校验：宏与宏、宏与显式普通动作的快捷键冲突仍拒绝，未放宽同一动作绑定自身的保留 Escape/输入合法性检查。当前 AAADAW 的重复显式动作按 registry 顺序解析；原厂探针只证明一个显式 Undo 映射优先于已有 Show undo history 映射，尚未测清各 Section、宏、用户动作与工厂默认值之间的全部优先级，因此 `INPUT-BIND-001` 继续 `in_progress`。

nightly+Cranelift Action List/Settings/映射回归 6/6 通过（496 项过滤）；随后 `aaadaw` 全部 502 项测试通过、0 跳过，Stable strict Clippy、fmt、diff 与 parity CSV/证据链接检查也通过。

## 2026-10-13 Main action 40182：Select all items

默认动作导出确认 Main Section 的 40182 名为 `Item: Select all items`，快捷键字段为空。AAADAW 将其注册为 `item.select-all`，可从 Action List 搜索并运行。它按轨道和时间顺序选中工程中的 Audio 与 MIDI items、更新活动项锚点，并清除 automation point 选择；不更改 item 数据、工程修订或 Undo/Redo 历史。零 items 时动作禁用。跨轨选区顺序、原厂运行时启用条件和实窗界面尚未通过新的 REAPER 会话单独核验，`ITEM-SELECT-001` 继续 `in_progress`。

回归 `select_all_items_action_selects_audio_and_midi_without_editing_the_project` 覆盖跨轨 Audio/MIDI 选择、稳定 action ID、Action List 搜索和 Run，以及工程/Undo 状态不变。加入相邻 40421 动作后，完整 `aaadaw` nightly+Cranelift nextest 为 504/504 通过、0 跳过；Stable strict Clippy、fmt 与 diff 检查通过。

## 2026-10-13 Main action 40421：Select all items in track

同一原厂 Main Section 导出中还列有 40421 `Item: Select all items in track`，快捷键字段为空。AAADAW 将其注册为 `item.select-all-in-track`，Action List Run 当前针对活动所选轨道选中其中所有 Audio 与 MIDI items，并替换此前跨轨选择；不修改工程或 Undo/Redo。REAPER 运行时的“active track”与多轨选择边界尚未核实，功能矩阵保持 `in_progress`。

回归 `select_all_items_in_track_action_keeps_other_tracks_unselected` 覆盖跨轨工程、Audio/MIDI 混合、Action List 搜索/Run、空默认键和其它轨道取消选择。动作选择专项 nextest 2/2 通过；后续全量测试与静态检查记录于进度文档。

## 2026-10-13 Main actions 40717/40718：时间选区内选择 items

默认动作导出中的 Main Section 列有 40717 `Item: Select all items in current time selection` 和 40718 `Item: Select all items on selected tracks in current time selection`，均无默认快捷键。AAADAW 分别注册为 `item.select-all-in-time-selection` 与 `item.select-all-on-selected-tracks-in-time-selection`，可搜索并通过 Action List Run 执行。两者按 item 区间与时间选区的半开区间相交来筛选 Audio/MIDI items；第二项范围由多选轨道和活动轨道组成。结果按轨道及时间排列，替换原 item 选择并清除 automation point 选择；不修改工程修订或 Undo/Redo 历史。没有时间选区或范围内没有 item 时禁用。

`select_all_items_in_time_selection_action_selects_overlapping_audio_and_midi` 与 `select_all_items_on_selected_tracks_in_time_selection_action_respects_multi_track_scope` 覆盖 Action List 搜索/Run、Audio/MIDI 混合、范围边界、轨道过滤、多轨范围和工程历史不变。Item 结束于时间选区起点或开始于选区终点时均排除，是目前采用的半开区间解释；REAPER 原厂对此边界及多选轨道运行时的实窗结果尚无独立探针，`ITEM-SELECT-003/004` 保持 `in_progress`。

## 2026-10-13 Main action 40289：Unselect all items

默认动作目录确认 Main Section 有 40289 `Item: Unselect (clear selection of) all items`，无默认快捷键。AAADAW 将其注册为 `item.unselect-all`；Action List Run 清除选中的 item 集合、活动 item 和 item 选择锚点，但保留轨道选择、时间选区及 automation point 选择，也不修改工程或 Undo/Redo 历史。没有选中 item 时动作禁用。

`unselect_all_items_action_clears_item_selection_and_preserves_other_view_state` 覆盖 Action List 搜索/Run、Audio/MIDI item 选择清除、轨道/时间范围保留、无默认快捷键和工程历史不变；`clearing_item_selection_preserves_track_time_and_automation_selection` 单独验证 timeline 选择状态边界。原厂运行时的启用条件及对 automation point 选择的交互尚未探测，`ITEM-SELECT-005` 保持 `in_progress`。

## 2026-10-13 Main action 40528：Select item under mouse cursor

默认目录确认 Main Section 有 40528 `Item: Select item under mouse cursor`，无默认快捷键。Arrange 当前在鼠标移动进入 item 区域时更新悬停 item，离开 item/时间线、载入不同工程或改变视图布局时清除目标；Action List Run 选择该目标并替换此前 item 选择，不修改工程修订或 Undo/Redo。视图平移/缩放后需要鼠标再次移动来重新命中；没有有效悬停 item 时动作禁用。

`arrange_cursor_motion_tracks_and_clears_the_hovered_item` 从真实 timeline shader 鼠标事件验证悬停命中/清除；`select_item_under_mouse_action_uses_current_hover_and_replaces_selection` 验证 Action List 搜索/Run 和工程历史不变。REAPER 原厂对 Action List 焦点、视图变换后悬停目标及实窗界面的行为尚无运行探针，`ITEM-SELECT-006` 保持 `in_progress`。

## 未完成项

Main 以外的 Section、完整默认动作与功能、动作导入导出、菜单编辑器、原厂自定义动作窗口、特殊键/Scope/MIDI/OSC 输入和完整重复绑定优先级仍未对齐。Add 自动提交已在后续修正。主题和几何尚未完成像素比较，矩阵保持 `in_progress`。

## CI 基础设施修复

先前 Android 模拟器运行 `37953099817` 的日志显示 `-accel off`、无 `/dev/kvm` 权限、系统级 ANR 和 CPU 压力；AAADAW 进程存活且截图检查已通过。工作流新增 KVM 权限配置与加速预检，显式启用硬件加速，保留原有前台与渲染断言；运行结果需在后续唯一 PR 的 CI 验证。

## 独立录入确认续项

Add 现在打开 387×229 的 Keyboard input 窗口，默认键入仅更新草稿；OK/Enter 确认后原子保存，Cancel/Escape 或关闭窗口丢弃草稿。保存失败保持窗口、组合和原配置。子窗口存在时屏蔽父列表的选择/筛选/Run/Find 操作及其他窗口快捷键，固定录入所属动作；关闭列表同步关闭录入。无工程 schema 变更。

774 项 workspace 测试全部通过，随后父窗口输入守卫修改的 6 项相关回归通过，Clippy warnings denied 与默认构建通过。GUI：给 Undo 录入 Alt+F12，尚未确认时没有 shortcuts.conf；Cancel 后仍无文件；重新录入并 Enter 后文件为 edit.undo → Mod+Z; Alt+F12，窗口关闭、列表同步更新。

![录入草稿](images/action-input-draft.png)
![确认保存](images/action-input-saved.png)

窗口只实现键盘基础输入；特殊键入口、Enter 本身的特殊录入方式、Scope、MIDI/OSC、自动关闭选项和原厂完整布局仍缺。重复分配优先级不能由单次实测推断，继续保持未完成。
