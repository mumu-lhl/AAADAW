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
- Filter 的 Enter 将焦点交给首个可见结果；结果列表的 ↑/↓ 移动选择、在首尾停止并在越出视口时滚动，Home/End 选择并滚动到首项/末项，Enter 执行当前选择。Linux REAPER 7.82 / Default 7 与 AAADAW 的对照截图、键盘路由回归见[输入焦点验证](input-focus.md#action-list-结果导航)。
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

默认动作导出确认 Main Section 的 40182 名为 `Item: Select all items`，快捷键字段为空。AAADAW 将其注册为 `item.select-all`，可从 Action List 搜索并运行。REAPER 7.82 原生探针在 3 轨、13 个混合 Audio/MIDI items 上选中全部 items；工程状态计数增加 1，Undo 标签变为 `Select all items`。AAADAW 同样选中混合 item 并把选择动作放进应用级 Undo/Redo 历史，但不增加内容修订。原厂窗口和逐项结果见[REAPER Arrange 截图](fixtures/item-selection-all-native.png)及[探针记录](fixtures/item-selection-actions-native.tsv)。新的 AAADAW 实窗验证用 Action List 搜索运行 40182；1280×1024 截图可见两条轨道上的两个 MIDI items 都被选中，Ctrl+Z 状态显示 `Undid Select all items`，Ctrl+Y 显示 `Redid Select all items`。Action List、选中、撤销、重做窗口图和二进制哈希见[AAADAW 验证记录](fixtures/item-selection-aaadaw.tsv)，截图分别为[Action List](fixtures/item-selection-action-list-aaadaw.png)、[选择结果](fixtures/item-selection-all-aaadaw.png)、[撤销](fixtures/item-selection-undo-aaadaw.png)和[重做](fixtures/item-selection-redo-aaadaw.png)。原厂内容保存/脏标记、automation point 交互及零 items 的启用状态尚未探测，`ITEM-SELECT-001` 继续 `in_progress`。

回归 `select_all_items_action_selects_audio_and_midi_without_editing_the_project` 覆盖跨轨 Audio/MIDI 选择和 Action List Run；`item_selection_undo_and_redo_stay_in_order_with_project_edits` 覆盖选择与内容编辑的撤销顺序。无变化动作和新 Undo 分支另有回归。原厂选择事件会增加状态计数，AAADAW 的选择历史暂时只保存在应用层；保存/脏标记语义仍需对齐。

## 2026-10-13 Main action 40421：Select all items in track

同一原厂 Main Section 导出中还列有 40421 `Item: Select all items in track`，快捷键字段为空。隔离 REAPER 7.82 实测：只选中轨道 A 时仅选择 A 上的 items；同时选 A+B 时无论轨道选择顺序如何，都选择 A 与 B 上的 items，工程状态计数增加 1，Undo 标签为 `Select all items in selected tracks`。AAADAW 已调整为按所有选中轨道（含活动选中轨道）选择，并把动作纳入应用级撤销历史。探针记录见[TSV](fixtures/item-selection-actions-native.tsv)。Item 菜单启用边界、focus 与保存语义尚待验证，矩阵继续 `in_progress`。

回归 `select_all_items_in_track_action_selects_all_selected_tracks` 覆盖多轨选择与 Audio/MIDI 混合；撤销顺序和 redo 分支由相邻测试覆盖。

## 2026-10-13 Main actions 40717/40718：时间选区内选择 items

默认动作导出中的 Main Section 列有 40717 `Item: Select all items in current time selection` 和 40718 `Item: Select all items on selected tracks in current time selection`，均无默认快捷键。AAADAW 分别注册为 `item.select-all-in-time-selection` 与 `item.select-all-on-selected-tracks-in-time-selection`，可搜索并通过 Action List Run 执行。原厂实测在时间选区 `[1,3]` 时，40717 选中从 1 开始、区间内部及跨过 3 的 Audio/MIDI items；结束于 1 或开始于 3 的 items 被排除，符合半开区间相交。40718 在此基础上只选中多选轨道 A 与 C 的 items。两项都增加一次工程状态计数并建立 Undo 标签 `Select items in time selection`。AAADAW 当前匹配选择集合及范围，并加入应用级撤销历史；持久化和 automation point 行为仍未探测。完整记录见[原厂 TSV](fixtures/item-selection-actions-native.tsv)。

`select_all_items_in_time_selection_action_selects_overlapping_audio_and_midi` 与 `select_all_items_on_selected_tracks_in_time_selection_action_respects_multi_track_scope` 覆盖 Action List 搜索/Run、Audio/MIDI 混合、边界和轨道过滤。原厂时间选区端点、多轨范围现已有实测；运行时启用条件、保存/脏标记、automation point 与实际 AAADAW 窗口仍未验收，`ITEM-SELECT-003/004` 保持 `in_progress`。

## 2026-10-13 Main action 40289：Unselect all items

默认动作目录确认 Main Section 有 40289 `Item: Unselect (clear selection of) all items`，无默认快捷键。原厂探针确认它清空全部 item 选择，同时保留轨道 C 选择、时间选区 `[1,3]` 和编辑光标 4.0；状态计数增加 1，Undo 标签为 `Unselect all items`，撤销后恢复原来的 13 个 item 选择。再次对空选择运行时状态计数增加 0，Undo 标签保持不变。对已有 redo 分支执行一次确实改变选择的 40289 会清除 redo 分支；无变化动作不清除它。AAADAW 的 `item.unselect-all` 已加入应用级撤销历史并按快照恢复 item/活动项/锚点，且 no-op 不建历史。

`unselect_all_items_action_clears_item_selection_and_preserves_other_view_state` 覆盖 Action List Run 和时间线状态边界；`no_op_item_selection_does_not_discard_redo_history` 与 `changed_item_selection_discards_project_redo_history` 覆盖 redo 语义。原厂 automation point、命令禁用状态、内容保存/脏标记仍未探测；AAADAW 的 automation point 交互尚无实窗对比，`ITEM-SELECT-005` 保持 `in_progress`。

## 2026-10-16 Main action 40528：Select item under mouse cursor

默认目录确认 Main Section 有 40528 `Item: Select item under mouse cursor`，无默认快捷键。隔离 Linux REAPER 7.82 / Default 7 原生探针确认：指针在 MIDI item A1 上运行会替换选择并选中 A1；指针在空白 item lane 上运行会清空已有 item 选择；指针在 Action List 窗口上运行时没有 Arrangement item target，选择保持不变。Action List 选中 action 后 Run 按钮仍可用；因此 Main action 不按当前 hover 是否命中 item 来禁用，空白轨道 item lane 和 Action List 外的无效 target 分别执行清空与 no-op。

40528 的 Action List 有效 Audio target 也已通过原厂实窗验证：初始选择 MIDI A1，把 Action List 的 `Item: Select item under mouse cursor` 结果选中后将真实指针停在 Audio B1 并按 Enter，选择由 A1 替换为 B1。`GetItemFromPoint()` 记录 B1，project state/Undo 与前后截图见 [40528 Audio target TSV](fixtures/item-select-actionlist-target-audio-native.tsv)、[运行前](fixtures/item-select-actionlist-target-audio-40528-before-native.png)和[运行后](fixtures/item-select-actionlist-target-audio-40528-after-native.png)。AAADAW 的 [Audio hover-selection 回归](../../../crates/aaadaw/src/app/tests.rs)现在也覆盖 40528 在 Audio B1 上替换既有 A1 选择。

原厂 probe 在 40529 上还确认 Add 对 B1 的二次执行不改变选择；40530 会移除已选 B1，但指针在空白 lane 时 no-op。完整逐次选择、project state/Undo 和指针坐标见 [原厂 TSV](fixtures/item-hover-actions-native.tsv)，可复现 ReaScript 为 [probe-item-hover-actions.lua](../../../scripts/reaper_parity/probe-item-hover-actions.lua)。[Action List 原厂截图](fixtures/item-hover-actions-action-list-native.png)显示 40528 已选中且 Run 可用；[运行后的 Arrange 截图](fixtures/item-hover-actions-action-list-run-native.png)对应 Action List 覆盖指针时选择保持不变。

AAADAW 的 40528 现在保持可运行：在真实 track item lane 上命中 item 时替换选择，在同一 lane 的空白处清空 item 选择，在 ruler、TCP 或离开 Arrange 后无效 target 时 no-op。清空和选择都进入应用级 item-selection Undo 历史，不改工程内容修订或 Project Undo 栈。`arrange_cursor_motion_tracks_and_clears_the_hovered_item` 验证真实 timeline shader 鼠标事件；`select_item_under_mouse_action_selects_item_clears_empty_lane_and_noops_outside_arrange` 覆盖 Action List Run、item 命中、空 lane 清空、Undo 恢复及 Arrange 外 no-op。平移、缩放和滚动后的 stationary-pointer 重命中由相邻 timeline regressions 覆盖。AAADAW 新桌面二进制的实窗验收通过 Action List 选中 40528、将鼠标停在 MIDI item 上并按 Enter 完成；截图及构建 ID/哈希在[AAADAW 运行 fixture](fixtures/item-hover-actions-aaadaw.tsv)和[实窗截图](fixtures/item-hover-actions-aaadaw.png)。`ITEM-SELECT-006` 保持 `in_progress`，因为 item-selection 其余分支与实窗覆盖仍未全部验收；40529/40530 的 no-target、range-anchor、track-focus 维度已验证，primary active-item identity 仍开放。

## 2026-10-16 Main actions 40529/40530：添加或切换鼠标下的 item 选择

默认动作目录列出 40529 `Item: Select item under mouse cursor (leaving other items selected)` 和 40530 `Item: Toggle selection of item under mouse cursor`，均无默认快捷键。REAPER 原生探针确认 40529 会添加悬停 B1，重复运行保持选择集合不变；40530 会移除已选 B1，空白 item lane 上 no-op。在零轨道、零 item 的工程中分别选中 40529 和 40530，Action List 的 Run 都保持可用；点击后工程仍为空。逐项观察和前/后截图见[availability TSV](fixtures/item-hover-actions-availability-native.tsv)：[40529 运行前](fixtures/item-hover-actions-40529-action-list-native.png)、[运行后](fixtures/item-hover-actions-40529-action-list-run-native.png)、[40530 运行前](fixtures/item-hover-actions-40530-action-list-native.png)和[运行后](fixtures/item-hover-actions-40530-action-list-run-native.png)。AAADAW 注册稳定 ID `item.add-under-mouse-to-selection` 与 `item.toggle-under-mouse-selection`；两者始终可运行，没有悬停 item 时 dispatch 为 no-op。

`select_item_under_mouse_leaving_other_items_selected_adds_without_toggling` 验证添加、重复执行、无目标可运行及 no-op；`toggle_selection_of_item_under_mouse_adds_and_removes_the_hovered_item` 验证 toggle 两个方向及无目标 no-op。新版 AAADAW 桌面二进制在空工程中运行 40529 和 40530 后保持空选择，Action List 的两个 Run 按钮均可用；窗口状态和构建信息见 [AAADAW availability fixture](fixtures/item-hover-actions-aaadaw-availability.tsv)、[40529](fixtures/item-hover-actions-aaadaw-40529-run.png)与[40530](fixtures/item-hover-actions-aaadaw-40530-run.png)。

原厂有效 hover target 的 Action List 路径也已验证。将原厂 Actions 窗口移动到 Arrangement 指针下方，选择 40529 后把真实指针停在 B1，再按 Enter，A1 变为 A1/B1；对 40530 重复后 B1 被移除，回到 A1。两次 `GetItemFromPoint()` 都是 B1，last-touched track 为 B；逐步状态和 Action List/Arrange 截图见 [valid-target Action List fixture](fixtures/item-hover-actionlist-target-native.tsv)、[40530 运行前](fixtures/item-hover-actionlist-target-40530-before-native.png)和[运行后](fixtures/item-hover-actionlist-target-40530-after-native.png)。这确认当 Action List 与 Arrangement 同时可见时，键盘 Run 会使用仍在 Arrange item 上的实际鼠标坐标。

原厂多选锚点通过真实 Shift-click 后续手势验证，因为 ReaScript API 只暴露所选 item 集合，不暴露内部 range anchor。先点击 A1 再 Shift-click B2，结果是 A1/B1/B2；恢复 A1 后运行 40529 把 A2 加入，再对 B2 做相同 Shift-click，结果是 A1/A2/B1/B2。40530 加入 A2 也保留 A1 锚点；40530 删除非锚点 A2 后范围仍为 A1/B1/B2；单选 A2 后用 40530 将其取消选择，选择集合变空，但随后的 Shift-click B2 仍得到 A2/B2。REAPER 7.82 Default 7 的状态记录、隔离配置和 SHA-256 截图见 [anchor fixture TSV](fixtures/item-hover-anchor-native.tsv)、[基准范围](fixtures/item-hover-anchor-baseline-shift-native.png)、[40529 添加后](fixtures/item-hover-anchor-40529-before-shift-native.png)与[Shift-click 后](fixtures/item-hover-anchor-40529-after-shift-native.png)。AAADAW 现在让 40529/40530 保留之前的 `selection_anchor`，测试还覆盖了 Toggle 移除锚点 item 后保留空选择中的锚点。track selection 与 last-touched-track 现已由下段原厂实测确认；MIDI Editor active take 与 MIDI item 的 Action List 有效 target 均已实测，不能据此推断存在 Arrange-level primary active item，`ITEM-SELECT-007/008` 仍 `in_progress`。

Track selection 和 last-touched-track 分开验证：只选择原厂 Track A 后，把指针停在 Track B 的 B1 并运行 40529，随后运行 40530 移除 B1；两次的 `selected_tracks` 都仍是 A，而 `GetLastTouchedTrack()` 从 A 变成 B。AAADAW 对应状态是 `selected_tracks={A}`、`selected_track=B`，App 回归测试覆盖 Add 和 Toggle 两项。逐步 API 记录和当时的实窗见 [selection-focus fixture TSV](fixtures/item-hover-actions-selection-focus-native.tsv) 与[原厂截图](fixtures/item-hover-actions-selected-track-A-item-B1-native.png)。`GetSelectedMediaItem` API 的枚举按工程顺序返回；多 item Properties 的差异 Name 为空，没有显示独立 primary item。

为观察原厂在多选后实际使用哪个 MIDI item，原厂探针给 A1/A2/B1/B2 分配不同音高，通过真实点击选择后运行 action 40153 `Item: Open in built-in MIDI editor`，并读取 `MIDIEditor_GetTake()`。点击 A1 后 Ctrl-click B1（B1 是最后点击项）时，MIDI Editor 仍以工程顺序第一项 A1、音高 60 为 active take；40529 在指针下添加 A2 后仍是 A1/60。另一次先选择 A2、B1、最后点击 A1，再由 40530 移除 A1，剩余 A2/B1 后 active take 成为 A2/62。完整操作和 API 值见 [active MIDI take TSV](fixtures/item-hover-active-midi-take-native.tsv)，三张 Default 7 实窗见[基准](fixtures/item-hover-active-midi-take-baseline-native.png)、[40529](fixtures/item-hover-active-midi-take-40529-native.png)和[40530](fixtures/item-hover-active-midi-take-40530-native.png)。这证明 MIDI Editor 使用所选集合中工程顺序最前的 take，并不证明 Arrange 有独立 active item。AAADAW 现在在 Toggle 移除当前 `selected_item` 后，以 Arrangement 工程顺序选取第一个剩余 item；新增多项回归覆盖 A1 被移除后选择 A2（而非任取 HashSet 项）。MIDI 与 Audio item 的有效 Action List target 现已分别确认；`ITEM-SELECT-007/008` 继续开放其它焦点边界。

补充验证 40529/40530 Audio target：使用 [1 秒 PCM 测试源](fixtures/item-hover-actionlist-target-audio.wav) 创建 Track B 的 Audio item B1，初始只选中 Track A 的 MIDI item A1。将原厂 Action List 放在 B1 下方，真实鼠标停在 B1 上，按 Enter 执行 40529 后选择变为 A1/B1；再以同样流程运行 40530 后恢复 A1。探针两次 `GetItemFromPoint()` 都返回 B1，逐项状态与 project state/Undo label 见 [Audio Action List TSV](fixtures/item-hover-actionlist-target-audio-native.tsv)；[40530 运行前](fixtures/item-hover-actionlist-target-audio-40530-before-native.png)显示选中的 Audio B1 和 Action List 结果，[运行后](fixtures/item-hover-actionlist-target-audio-40530-after-native.png)显示 B1 已取消选择。原厂探针由 [probe-item-hover-actionlist-target.lua](../../../scripts/reaper_parity/probe-item-hover-actionlist-target.lua) 执行，通过 `AAADAW_ACTIONLIST_TARGET_AUDIO_WAV` 指向该 WAV；AAADAW 回归 `hover_selection_actions_select_add_and_toggle_audio_items` 覆盖 Audio item 的 Select/Add/Toggle 与既有 MIDI 选择、last-touched track 及 selected-track set。

AAADAW 桌面实窗也以 Arrange 中的 Audio B1 作为 40529 target：在 Action List 搜索并选中该动作、将鼠标停在 B1 后按 Enter，运行前截图中 B1 没有选择边框，运行后出现橙色选择边框。1280×800 前后截图、Audio A1/B1 fixture、测试源 WAV 与二进制 Build ID/SHA-256 见 [AAADAW Audio Add fixture](fixtures/item-hover-actionlist-audio-add-aaadaw.tsv)、[运行前](fixtures/item-hover-actionlist-audio-add-aaadaw-before.png)和[运行后](fixtures/item-hover-actionlist-audio-add-aaadaw-after.png)。该实窗结果补充了原厂 40529/40530 Audio target 对照；40530 的 AAADAW 有效目标仍只有 App 回归覆盖，本组桌面截图没有验证 40530。

随后发现 AAADAW 的 Arrangement 横向平移或缩放会清除 hover 缓存，导致指针停在原处时 Action List 动作失去目标。现在 Arrangement 保留视口内的指针坐标，并在滚轮/中键平移或缩放、垂直滚动及轨道行布局重建后重新命中该坐标下的 item；`stationary_arrange_pointer_retargets_items_after_pan_and_zoom` 覆盖从一个 item 移入另一个 item，以及缩放后指针落入空隙的情况，`stationary_arrange_pointer_retargets_items_after_vertical_scroll` 覆盖滚动后切换到另一轨道下的 item。该修复覆盖 AAADAW 视图变化后的 stale target；原厂 range anchor、track selection 与 last-touched-track、无目标启用状态、MIDI Editor active-take 顺序及 40529/40530 的 MIDI/Audio Action List 有效 target 均已验证；40528 专属的有效目标组合和其它焦点边界仍保持 `in_progress`。

## 2026-10-13 Main action 41115：Invert selection

Main Section 动作导出确认 41115 的描述为 `Item: Invert selection`，没有默认快捷键。隔离 REAPER 7.82 / Default 7 原生探针确认：空选择会选择所有 Audio/MIDI items；部分选择会得到全工程补集；全选会清空选择。结果顺序沿用工程/时间顺序，轨道选择和编辑光标不变。每次改变选择都会使工程状态计数增加 1，并把 Undo 标签设为 `Invert item selection`。探针还通过 Main_OnCommand 在零 item 工程中强行调用该动作；这类 API 调用不能证明原厂 Action List 在该状态下是否启用。

AAADAW 将选择变化放在应用级 Undo/Redo 历史中，内容修订和 Project 内容 Undo 栈保持不变。`invert_item_selection_action_selects_the_complement_of_audio_and_midi_items` 覆盖空选择、全选和部分选择、Action List 搜索/Run、混合类型、撤销/重做及重复执行。原厂 Action List 的可用状态、时间线“活动 item”规则和实窗焦点仍待实测，`ITEM-SELECT-009` 保持 `in_progress`。原厂逐项输出与可复现脚本见 [native TSV](fixtures/item-invert-selection-native.tsv) 和 [ReaScript](../../../scripts/reaper_parity/probe-item-invert-selection.lua)。

## 2026-10-13 Main actions 41127/41128：相邻非重叠 item 选择

Main Section 动作导出列出 41127 `Item: Select next adjacent non-overlapping item` 和 41128 `Item: Select previous adjacent non-overlapping item`，均无默认快捷键。隔离 REAPER 7.82 的真实 item 点击探针显示，目标边界必须与源边界精确相接；存在时间空隙时不算 adjacent。多选时每个源 item 独立映射到同轨相邻 item，没有目标的源项会从结果里移除；整个集合没有目标时，原厂仍执行动作并清空 item 选择。原厂动作不移动 edit cursor。AAADAW 已按 Audio sample 到项目 tick、MIDI tick 统一比较，并保留轨道选择和项目/Undo 历史。

`adjacent_non_overlapping_item_actions_navigate_within_the_active_track` 与 `adjacent_item_actions_require_touching_boundaries_and_map_each_selection` 覆盖边界相接、跨类型、重叠/空隙过滤、多选逐项映射、无目标清空及工程/Undo 历史不变。原厂运行记录见 [item navigation TSV](fixtures/item-navigation-native.tsv)；空选择时动作可用性、完整 Mouse/Action List 焦点及 7.82 实窗视觉验收仍未关闭，`ITEM-SELECT-010/011` 保持 `in_progress`。

## 2026-10-13 Main actions 41167/41168：移动光标到最近 item 边缘

Main Section 动作导出列出 41167 `Item navigation: Move cursor left to nearest item edge` 与 41168 `Item navigation: Move cursor right to nearest item edge`，均无默认快捷键。原厂 7.82 探针确认两项搜索整个工程的 Audio 与 MIDI item 起止边缘，不依赖 item 选择；边缘必须严格位于当前 edit cursor 的左侧或右侧。光标正好位于一个边缘时会跳过该点，继续寻找下一个边缘。AAADAW 按连续项目采样位置比较候选边缘，Audio 边缘保持精确样本、MIDI 边缘使用 tick；执行只移动光标，保留 item 选择和项目/Undo 历史。

`item_edge_navigation_actions_move_to_nearest_audio_or_midi_boundary` 现在以非 tick 对齐 Audio 边界测试混合类型、Action List Run、双向导航、无目标、严格跳过相等端点、item 选择保留和工程历史不变；原厂音频边缘、MIDI 边缘、无选择及相等端点输出见 [item navigation TSV](fixtures/item-navigation-native.tsv)。REAPER 7.82 对一个真实 PCM Audio take 的 start sample `192012` 与 end sample `240012` 均返回精确边缘：41167/41168 也精确命中 start sample；四个动作在 Snap on/off 下都一致，project state delta 为 0。秒数、sample、运行环境和原厂 Arrange 截图见 [native cursor fixture](fixtures/item-cursor-sample-native.tsv)。Action List 焦点和整套 Arrange 实窗对照仍未完成，`ITEM-SELECT-012/013` 保持 `in_progress`。

## 2026-10-13 Main actions 41139/41140：相邻轨道 item 导航

Main Section 动作 41139 `Item navigation: Select and move to item in previous track without changing track selection` 和 41140 `Item navigation: Select and move to item in next track without changing track selection` 已由隔离官方 7.82 / Default 7 运行探针验证。动作以按轨道、时间排序后的首个所选 item 为源，只查找项目顺序中紧邻的上一/下一轨；目标是该轨上第一个与源 item 时间区间有正长度重叠的 item。成功后只选中目标、把 edit cursor 移到目标起点，并保留轨道选择。邻轨没有重叠 item 时不会跳过空轨寻找更远轨道，而是清空 item 选择且不移动光标。多选时只使用首个所选源 item。

AAADAW 注册稳定 ID `item.select-previous-track` 与 `item.select-next-track`。`adjacent_track_navigation_uses_first_overlapping_item_and_preserves_track_selection` 覆盖紧邻空轨、相邻轨多个重叠目标、多选首项、光标、Action List Run、轨道选择与项目/Undo 状态。原厂点击驱动记录与可复现脚本见 [item navigation TSV](fixtures/item-navigation-native.tsv) 和 [probe](../../../scripts/reaper_parity/probe-item-navigation.lua)。空选择时动作可用性、完整 Action List 键盘焦点与 Arrange 视觉验收仍在进行，`ITEM-SELECT-014/015` 不标记完成。

复现时需在独立 REAPER 7.82 / Default 7 配置的空白、未保存工程中运行脚本；它会创建自有轨道/item 并写入 `/tmp/aaadaw-item-navigation-ready`。在 Arrange 中点击源 item 后，向 `/tmp/aaadaw-item-navigation-request` 写入 `测试名<TAB>动作编号`；可选第三列传入要先设置的 edit-cursor 秒数。结果追加到 `/tmp/aaadaw-item-navigation-native.tsv`，写入 `done<TAB>0` 结束采样。脚本对版本、主题、播放状态及空工程作断言，不保存该项目。

## 2026-10-13 Main actions 41173/41174：所选 items 边界光标导航

Main Section 动作目录列出 41173 `Item navigation: Move cursor to start of items` 与 41174 `Item navigation: Move cursor to end of items`。REAPER 7.82 / Default 7 真实点击探针确认：单选时分别移动到该 Audio 或 MIDI item 的起点/终点；多选时 41173 使用所有所选 items 的最早起点，41174 使用最晚终点，且范围可跨轨。空 item 选择时两项均不移动光标；执行不修改 item/track 选择或工程状态。

AAADAW 已加入稳定动作 ID `item.navigate-cursor-selected-start` 与 `item.navigate-cursor-selected-end`，分别移动到所选 Audio/MIDI 区间的最早与最晚边界。Audio 边界以项目 sample 精确保留，MIDI 边界保留项目 tick；Arrange 光标 shader 使用分数 tick 定位，播放 seek、Audio 导入、Loop 粘贴、光标处分割和 Nudge/Set 光标目标读取同一精确位置。`selected_item_cursor_actions_use_outer_audio_midi_bounds` 覆盖跨轨混合选择、未选项过滤、选择/工程/Undo 保持；`selected_audio_item_cursor_actions_preserve_sample_exact_boundaries` 覆盖非 tick 对齐 Audio 起止样本、光标位置、导入、光标处分割和普通 tick 光标重置；`item_edge_navigation_actions_move_to_nearest_audio_or_midi_boundary` 也使用非 tick 对齐 Audio 边界。原厂逐项结果见 [item navigation TSV](fixtures/item-navigation-native.tsv)；可用上文探针脚本在真实点击一个 item 后追加 `41173`/`41174` 请求，多选使用 Ctrl-click。

AAADAW 的 Main 41173 实窗操作已通过 Action List 完成：选择 Track B 的 Audio item 后运行 `Item navigation: Move cursor to start of items`，Transport 到达 `0:04.000`，Item Inspector 保持显示精确起点 sample `192012`，Arrange 光标落在该 item 起边界。运行前后 Action List 与 Arrange 截图、1280×800 Xorg 配置、Build ID、binary/screenshot hashes 和可重建的 off-grid 项目见 [AAADAW cursor fixture](fixtures/item-cursor-sample-aaadaw.tsv)、[运行前](fixtures/item-cursor-sample-before-aaadaw.png)、[Action List 运行前](fixtures/item-cursor-sample-actionlist-before-aaadaw.png)、[Action List 运行后](fixtures/item-cursor-sample-actionlist-after-aaadaw.png)和[Arrange 结果](fixtures/item-cursor-sample-after-aaadaw.png)。

Main 41174 也已通过 AAADAW 实际 Action List 执行：选中同一 Audio item 后运行 `Item navigation: Move cursor to end of items`，Transport 到达 `0:05.000`，所选项仍在 Inspector 中显示起点 sample `192012`。运行前后 Action List 和滚动后可见 Track B 的 Arrange 证据、binary/hash 与运行配置见 [41174 fixture](fixtures/item-cursor-end-aaadaw.tsv)、[运行前](fixtures/item-cursor-end-before-aaadaw.png)、[Action List 运行前](fixtures/item-cursor-end-actionlist-before-aaadaw.png)、[Action List 运行后](fixtures/item-cursor-end-actionlist-after-aaadaw.png)与[Arrange 结果](fixtures/item-cursor-end-after-aaadaw.png)。

REAPER 原厂 off-grid Audio 精度已按真实 PCM take 验证：Main 41173/41174 在 Snap on/off 下分别返回 sample 192012/240012，`GetProjectStateChangeCount()` 增量为 0；精确秒数与探针见 [native cursor fixture](fixtures/item-cursor-sample-native.tsv)。AAADAW 空 item 选择时 41174 的 Action List 结果仍可运行，执行后 Transport 保持 `0:06.000` 且 Inspector 无选择项；前后实窗与二进制信息见 [empty-selection fixture](fixtures/item-cursor-empty-selection-aaadaw.tsv)。原厂无选择时动作语义已由 [item navigation TSV](fixtures/item-navigation-native.tsv) 验证，但原厂 Action List 可用状态截图、完整窗口焦点和 Arrange 视觉对照尚未完成；因此 `ITEM-SELECT-016/017` 仍保持 `in_progress`。

## 2026-10-13 Main actions 41182/41183/41184：所选 items 网格对齐

Main Section 动作目录列出 41182 `Item: Snap items left`、41183 `Item: Snap items right` 和 41184 `Item: Snap items to nearest snap point`，默认均无快捷键。REAPER 7.82 / Default 7 的 Arrange 点击探针确认三项会分别对每个选中 Audio/MIDI item 的起点执行向左、向右或最近网格点对齐；多选逐项独立对齐，不施加共同位移。Audio/MIDI item 的长度保留，item/track 选择和 edit cursor 保持不变。已在网格上的起点不移动；最近点等距时取左侧网格点。即使全局 Snap toggle 关闭，这三项仍使用项目网格。混合 Audio/MIDI 多选可由一次 Undo 完整还原。

AAADAW 注册稳定 ID `item.snap-left`、`item.snap-right` 与 `item.snap-nearest`，使用当前 Arrangement 网格和 tempo map；Audio 位置通过 tick 网格点映射为精确 sample，再按 sample 距离选择方向，避免先将 item 位置量化到 tick。批量编辑作为单一事务写入，未移动时不创建历史。`item_snap_actions_match_audio_and_midi_grid_directions_with_grouped_undo`、`item_snap_nearest_uses_sample_grid_and_ignores_snap_toggle` 和 `item_snap_exact_grid_empty_selection_and_busy_states_do_not_edit` 覆盖 Action List Run、左右/最近/平局、独立混合类型移动、长度与 source offset 保留、Snap toggle 关闭、多选 Undo/Redo、精确网格、空选择及 project-edit busy 守卫。AAADAW 在 playback active 时阻止工程修改；这三项原厂动作在播放期间的可用性尚未实测。原厂证据和复测脚本见 [item snap TSV](fixtures/item-snap-native.tsv) 与 [probe](../../../scripts/reaper_parity/probe-item-snap.lua)。

探针以四分音符直网格验证；Swing 网格、播放期间的原厂动作状态及完整 Arrange/Action List 视觉和焦点尚未比对，`ITEM-SELECT-018/019/020` 仍为 `in_progress`，不能据此视为整体 Item parity 完成。

## 2026-10-16 Main actions 41182/41183/41184：Swing 网格复测

用隔离的原厂 Linux REAPER 7.82 / Default 7、120 BPM、4/4、四分音符项目网格运行真实 Main actions，分别对 Audio 和 MIDI item 覆盖直网格、25%、50%、100%、−50% swing，Snap toggle 开/关，九个起点时间及 41182/41183/41184，共 540 行结果。原厂表格与隔离可复现脚本为 [swing TSV](fixtures/item-snap-swing-native.tsv) 和 [probe](../../../scripts/reaper_parity/probe-item-snap-swing.lua)。REAPER 二进制 SHA-256 为 `9976f5608068b7a58518969001b00b527fb0de6ed7459e0e5dda6f29878ee254`；TSV SHA-256 为 `877e9603e054f17fc4096837c89f76e5ad8c6a54f6c1452c87d9984bda4108b6`。设置窗口截图记录的是探针配置的 swing enabled / 50%，并非首次启动默认状态：[原厂 Snap/Grid Settings](fixtures/item-snap-grid-settings-swing-enabled-native.png)，SHA-256 `c5976e92a7aa3650301df041e24ac62c9213888043007125aceaee1c5a6cf5d`。

#### 2026-10-16 MIDI item fractional PPQ positions

The 540-row probe above uses swing values whose quarter-grid offsets happen to be integer PPQ ticks. A separate Linux REAPER 7.82 / Default 7 probe covers 90 MIDI-item cases: 0%, 1%, 3%, 7%, 17%, 25%, 33%, 50%, 100%, and negative swing values; Snap on/off; and Main actions 41182/41183/41184. At 120 BPM on a quarter-note grid, +1% swing moves an item from 0.500000000s to 0.502500000s (project position 964.8 PPQ) with action 41183; −1% plus action 41184 moves it to 0.497500000s (955.2 PPQ). The 0.0625s item length and selection remain unchanged. These fractional positions are native REAPER results, not rounding artifacts from the probe.

The reproducible [ReaScript](../../../scripts/reaper_parity/probe-item-snap-subtick.lua) checks the factory tempo and meter, explicitly selects a MIDI item, records project QN-derived PPQ, and writes the 90-row [native TSV](fixtures/item-snap-subtick-native.tsv). REAPER binary SHA-256: `9976f5608068b7a58518969001b00b527fb0de6ed7459e0e5dda6f29878ee254`; TSV SHA-256: `c489b72bdb0887aa0c03c66dab9510e829d012774d1f22d07cfaa1d32be21d01`.

AAADAW now represents MIDI item starts in exact 1/200-PPQ sub-ticks, which losslessly represents the integer swing percentages and integer grid intervals observed here. `SetMidiItemPosition` preserves fractional starts through project undo/redo, schema-56 persistence, Arrangement geometry, live MIDI sample scheduling, SMF output conversion, and MIDI Item Properties Apply/Undo/Redo. The Samples property round trip and App-level property edit cover 3840.8 and 3860.8 PPQ. An AAADAW-only Xvfb screenshot now confirms that selecting the 964.8-PPQ fixture displays `Tick 964.8` in the Arrangement Inspector; its reproducible project generator, binary hash and screenshot hash are in [the fixture record](fixtures/midi-item-fractional-inspector-aaadaw.tsv). Core, storage, app, playback and export regressions cover both native target values. The matrix rows remain `in_progress` because project save behavior in REAPER, playback-time Action List availability, ordinary Arrangement/MIDI Editor gestures, full-window visual/focus parity, and a direct screenshot comparison of fractional item placement have not been accepted.

观测结果是奇数序号网格点按 `swing amount × 网格间隔 ÷ 2` 做有符号偏移，偶数点保持直网格；最近点平局取左。原厂三项动作不依赖全局 Snap 开关；Audio/MIDI 目标相同。非零小节起点不作为这三项动作的候选点，包括直网格和 swing 网格，因此位于 4/4 小节线的 item 会向相邻非小节线网格点移动。AAADAW 新增 `Options > Snap/grid settings...` / `Alt+L` 设置窗、Swing grid 开关、−100% 至 100% 滑块和 Action List 的 `Grid: Toggle swing grid`，并让 41182/41183/41184 按此网格计算目标。定向回归覆盖 25/50/100/−50%、左右/最近、小节线、Snap 关闭、Audio/MIDI、选择及分组 Undo/Redo。

MIDI item 起点现用 1/200 PPQ 子刻度表达，覆盖原厂探针产生的所有百分比目标；子刻度随 MIDI source event 映射进入回放与导出。MIDI note 自身的 tick/duration 仍为整数 PPQ，普通 Arrange item 拖动/trim、时间选区、编辑光标、loop/region 边界、自动化点，以及 MIDI Editor 插入/移动吸附和 Swing 线虽已使用同一网格，仍需与 REAPER 原厂做鼠标/编辑器操作对照。针对性回归核对 50% 与负 swing、平局向左、拖动/trim 目标、MIDI note anchor 与绘制网格一致，以及 Snap/Shift 绕过。

Snap/grid/swing 现在按项目保存：schema 55 的 `arrangement_view_meta` 持久化设置，schema 56 的 `items.start_tick_subtick` 保存精确 MIDI item 起点；schema 55 工程迁移后子刻度为 0，未丢失旧工程位置。设置更新会进入项目 revision 和正常保存/未保存 session snapshot。项目保存/重开和旧工程迁移有 storage/session 回归；REAPER 原厂跨会话设置行为仍未单独探测。新 binary 在 Xvfb :96 的 [Arrange smoke 截图](fixtures/item-snap-grid-desktop-smoke.png)显示 Snap 和 1/4 Grid 控件， [fixture](fixtures/item-snap-grid-desktop-smoke.tsv)记录 binary/hash；这只是启动渲染检查，不是项目保存/重开证据。播放期间原厂 Action List 可用状态、完整实窗视觉与键盘焦点尚未对齐，三条矩阵行继续 `in_progress`。

这次传播改动的 AAADAW binary tests 通过 684/684（nightly Cranelift，保留断言）；Stable all-target Clippy `-D warnings`、fmt 与 `git diff --check` 通过。该结果验证实现回归，不替代尚未完成的对应 REAPER 鼠标/编辑器实测。

AAADAW 新 desktop binary 也已在 1280×800 Xorg :112 中实际启动。Alt+L 打开独立 `Snap/Grid Settings` 窗；初始窗口显示 `1/4`、Swing unchecked、`50%`，实际点击后 checkbox 状态更新；Arrange toolbar 可见 `Grid settings...` 按钮。运行时窗口截图、目标 binary SHA-256 与运行参数见 [AAADAW window fixture](fixtures/item-snap-grid-settings-aaadaw.tsv)，图像分别为[设置初始状态](fixtures/item-snap-grid-settings-aaadaw.png)、[Swing enabled](fixtures/item-snap-grid-settings-swing-enabled-aaadaw.png)和[Arrange toolbar](fixtures/item-snap-grid-toolbar-aaadaw.png)。本次只验证入口、控件状态变化与窗口可运行性，不将局部截图等同于整体视觉验收。

## 2026-10-14 Main action 40697：依焦点移除对象

Linux REAPER 7.82 / Default 7 的默认动作导出 `linux-7.82/actions.tsv` 记录 Main action 40697 `Remove items/tracks/envelope points (depending on focus)` 的默认键为 Delete；相邻的 40006 `Item: Remove items` 没有默认键。隔离 REAPER 与 AAADAW 的 701×475 Action List 窗口截图保存在[参照](fixtures/delete-by-focus-action-list-native.png)和[当前 AAADAW](fixtures/delete-by-focus-no-prompt-aaadaw.png)。两边均显示主动作与无提示变体；默认动作 TSV 只为主动作给出 40697，因此无提示变体使用 AAADAW 稳定 ID `edit.delete-focused-no-prompting`，不绑定默认键。Item 菜单命令 `Remove items` 也不绑定快捷键，Delete 只绑定 `edit.delete-focused`，不再错误显示 Delete/Backspace。

AAADAW 现按最近一次目标区交互路由 Delete：Arrange 的 item/空白区选择指向 items，TCP/MCP 轨道选择指向 tracks，automation 点选择指向 automation point。TCP/MCP 名称默认显示为不可编辑标签；单击名称选轨，双击进入文本编辑，编辑时文本控件按自身焦点处理 Delete。轨道面板获得目标后，普通 Delete 显示 `Delete N selected track(s)?` 确认框；Cancel、Escape 或点遮罩不改工程，确认后依赖轨道先删，整组只占一步 Undo。`edit.delete-focused-no-prompting` 则直接执行同一轨道事务。两种入口都只作用于当前目标区，不再按“有 item 就优先”跨区猜焦点。

隔离 REAPER 探针选中一条轨道和一个 MIDI item 后，点击 TCP 并按 Delete，原厂弹出 `Delete 1 selected track?`；取消时项目没有变化。保持 TCP 目标再从 Action List Run 无提示变体，轨道与 MIDI item 被直接移除，探针状态从 1/1 变成 0/0，原厂项目状态只新增一次。见 [原厂确认框](fixtures/delete-track-confirmation-native.png)、[原厂无提示执行后](fixtures/delete-track-no-prompt-native.png) 和 [运行时 TSV](fixtures/delete-by-focus-runtime-native.tsv)。AAADAW 的 [确认框](fixtures/delete-track-confirmation-aaadaw.png) 和[无提示执行后](fixtures/delete-track-no-prompt-aaadaw.png) 截图也由 Linux 主窗口实窗采集；应用回归覆盖 Arrange/TrackPanel 目标切换、混合 item/track 选择、取消/确认、无提示删除、原子 Undo/Redo 和 automation point 删除。

当前焦点模型是 AAADAW 主 Arrangement/Mixer 内最后一次 item、track 或 automation 目标选择，不等价于所有 REAPER 子窗口、插件控件、文本编辑框的真实键盘焦点；其他窗口与 TCP/MCP 非选择控件的焦点优先级仍待探测。Action List 完整默认目录、Main 外 Section、特殊键/Scope/MIDI/OSC、导入导出与全量窗口视觉对齐仍未完成，`INPUT-BIND-001` 继续 `in_progress`。

## 未完成项

Main 以外的 Section、完整默认动作与功能、动作导入导出、菜单编辑器、原厂自定义动作窗口、特殊键/Scope/MIDI/OSC 输入和完整重复绑定优先级仍未对齐。Add 自动提交已在后续修正。主题和几何尚未完成像素比较，矩阵保持 `in_progress`。

## CI 基础设施修复

先前 Android 模拟器运行 `37953099817` 的日志显示 `-accel off`、无 `/dev/kvm` 权限、系统级 ANR 和 CPU 压力；AAADAW 进程存活且截图检查已通过。工作流新增 KVM 权限配置与加速预检，显式启用硬件加速，保留原有前台与渲染断言；运行结果需在后续唯一 PR 的 CI 验证。

## 独立录入确认续项

Add 现在打开 387×229 的 Keyboard input 窗口，默认键入仅更新草稿；OK/Enter 确认后原子保存，Cancel/Escape 或关闭窗口丢弃草稿。保存失败保持窗口、组合和原配置。子窗口存在时屏蔽父列表的选择/筛选/Run/Find 操作及其他窗口快捷键，固定录入所属动作；关闭列表同步关闭录入。无工程 schema 变更。

774 项 workspace 测试全部通过，随后父窗口输入守卫修改的 6 项相关回归通过，Clippy warnings denied 与默认构建通过。GUI：给 Undo 录入 Alt+F12，尚未确认时没有 shortcuts.conf；Cancel 后仍无文件；重新录入并 Enter 后文件为 edit.undo → Mod+Z; Alt+F12，窗口关闭、列表同步更新。

![录入草稿](images/action-input-draft.png)
![确认保存](images/action-input-saved.png)

窗口目前只实现键盘基础输入；Scope、MIDI/OSC、自动关闭选项和原厂完整布局仍缺。REAPER 的 `Special key (Enter, Tab, etc.)` 入口现已实现：在该模式按 Enter 会把 Enter 写入快捷键草稿并保持窗口打开，只有之后点击 OK 才保存。原厂 7.82 和 AAADAW 的 387×229 实窗截图及步骤见 [special-key fixture](fixtures/keyboard-special-key-aaadaw.tsv)，AAADAW 另有 Enter 录入、保存后的 Action List 截图。Action List 显示 `Enter`，配置仍以 `StandardEnter` 保留主键区身份。功能回归覆盖 Enter、Tab、草稿替换、Enter 不误确认及保存。Named key 的常规捕获与此特殊键入口相互独立。重复分配优先级不能由单次实测推断；Section、MIDI/OSC、Scope、自动关闭与完整界面仍保持未完成。

## 2026-10-17 Named shortcut keys

Keyboard shortcut capture now accepts all 295 non-modifier `Named` keys exposed by iced 0.14, and their names survive config serialization and reload. This includes F25–F35 and browser/media key identities in addition to the previously supported function, navigation, and numeric-keypad keys. The pure shortcut regression checks every accepted `Named` value with Ctrl+Alt+Shift+Super. The live Action List/Keyboard input capture of F35 is recorded in [the AAADAW fixture](fixtures/action-list-named-key-aaadaw.tsv) and [screenshot](fixtures/action-list-named-key-aaadaw.png); it verifies input capture only and is not a native comparison. The REAPER Special keys chooser, Scope, MIDI/OSC input, default-action and cross-section priority rules remain open; no new native probe was run.

## 2026-10-17 Action List shortcut-map import and export

The Action List `Options` button now exposes `Import key map...` and `Export key map...`. AAADAW uses a versioned `AAADAW_SHORTCUT_MAP` TSV format with stable AAADAW action IDs and the existing canonical shortcut syntax. Export writes only user overrides; imported maps replace that override layer, while actions omitted from the file return to their factory bindings. Custom actions/macros are not serialized as definitions.

Import rejects unsupported versions, malformed or duplicate rows, invalid shortcuts, and unknown action IDs. Reads are bounded to 1 MiB. A valid map is fully checked against current action and macro definitions before the existing shortcut config is atomically replaced; parse, validation, or save failure leaves the active mapping unchanged. Export also writes through the existing atomic-config helper and refuses to replace the active `shortcuts.conf`.

The live Linux desktop capture shows the actual 701×475 Action List with the Options menu expanded; binary ID, screenshot hash, dimensions, and exact import/export semantics are recorded in [the fixture](fixtures/action-list-keymap-import-export-aaadaw.tsv). The four map parser/writer tests and both transactional import tests passed, followed by the full AAADAW desktop test suite at 704/704 and a Stable `cargo check`/desktop build. This Xvfb environment has no `xdg-desktop-portal` or `zenity`, so the platform file chooser could not be completed by pointer automation; the file-format, atomic-write, and state-replacement paths were exercised through tests.

The AAADAW-native map remains the portable format for the full AAADAW override set. REAPER interchange is documented separately below and remains a strict subset; `INPUT-BIND-001` remains `in_progress`.

## REAPER 7.82 `.ReaperKeyMap` Main-section subset

The Linux REAPER 7.82 native `Actions > Key map...` menu was exercised in an isolated Xvfb profile. A REAPER-exported Main-section file records each binding as `KEY <modifier-flags> <Windows virtual-key> <REAPER action ID> <section ID>`, followed by an optional comment. The fourth numeric field is the section ID (`0` for Main); it is not the separate keyboard Scope control. The exported sample in [the native fixture](fixtures/reaper-keymap-main-export-native.ReaperKeyMap) records the observed flags for Logo, Shift, Ctrl, Alt, and Ctrl+Alt+Shift on F11/F12, plus Ctrl+Z for action 40029 (`Edit: Undo`). The observed virtual-key values are Z=90, F11=122, and F12=123. The modifier flags are base bit 1, Shift bit 4, Ctrl bit 8, Alt bit 16, and Logo bit 32.

AAADAW now exposes separate `Import REAPER Main...` and `Export REAPER Main...` options alongside its versioned AAADAW map. The native reader/writer is deliberately strict: it supports Main-section keyboard `KEY` records only for 102 verified 7.82 actions, rejects unknown action IDs, other sections, scripts/custom-action records, unknown flags and unrepresentable keys, and leaves the current shortcut configuration unchanged when parsing or persistence fails. Export refuses empty overrides or actions/keys it cannot express rather than silently dropping them. The native file contains explicit user overrides; native import adds those bindings while retaining REAPER factory bindings.

The import path was tested in the REAPER UI using the AAADAW-format fixture [here](fixtures/reaper-keymap-main-import-aaadaw.ReaperKeyMap). After importing `KEY 17 122 40029 0`, the Main Action List showed `Ctrl+Z, Alt+F11` for Edit: Undo; the native screenshot is [here](fixtures/reaper-keymap-main-import-native.png). The original Key map import/export menu is [also captured](fixtures/reaper-keymap-native-menu.png). AAADAW's updated Options menu with all four map actions is captured in [the AAADAW screenshot](fixtures/action-list-keymap-options-aaadaw.png). Native REAPER binary SHA-256, both native screenshot hashes, AAADAW binary/build identity and the exact probe steps are recorded in the [native](fixtures/reaper-keymap-native.tsv) and [AAADAW](fixtures/action-list-keymap-import-export-aaadaw.tsv) fixture TSVs.

The native mapping table now contains 102 verified Main actions. It covers file/render commands, transport, loop and time-selection controls, project markers and regions, track controls, and item navigation/editing. A six-action file covering Show action list, Snap/grid settings, Insert new track, and selected-track mute/arm/solo was imported in REAPER; filtering the native Action List to Show action list displayed F2 alongside its factory `?` binding. A separate four-action file was accepted by REAPER and showed the new F9 binding for Item edit: Nudge/Set alongside factory N; see the [editing-action fixture](fixtures/reaper-keymap-main-editing-actions.ReaperKeyMap) and [screenshot](fixtures/reaper-keymap-main-editing-actions-native.png). The earlier [multi-action fixture](fixtures/reaper-keymap-main-import-aaadaw-multi.ReaperKeyMap) remains available. All 102 native IDs and names match the captured 7.82 Main catalog. Other AAADAW actions remain unmapped until their native identity and behavior are verified.

This is validated interoperability for the mapped Main keyboard subset, not full native map parity. The bundled German/DK layout files contain `KEY` entries with action ID zero, and native exports can include `SCR` records; those are intentionally rejected until AAADAW supports REAPER keyboard-layout remaps and custom script actions. Other Main action IDs, MIDI/OSC, non-Main Sections, Scope, macros as native actions, and complete factory mapping remain open. `INPUT-BIND-001` remains `in_progress`.
