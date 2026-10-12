# Project Markers — Linux REAPER 7.82

## 原厂证据

默认 Main section 的原厂动作目录记录了：

| Action ID | 原厂名称 | 默认键 |
| --- | --- | --- |
| 40157 | Markers: Insert marker at current position | `M` |
| 40172 | Markers: Go to previous marker/project start | `[` |
| 40173 | Markers: Go to next marker/project end | `]` |
| 40613 | Markers: Delete marker near cursor | 无；删除光标前后严格小于 1 秒的最近 Marker |
| 40656 | Markers: Add/move marker 10 to play/edit cursor | `Ctrl+0` |
| 40657 | Markers: Add/move marker 1 to play/edit cursor | `Ctrl+1` |
| 40658 | Markers: Add/move marker 2 to play/edit cursor | `Ctrl+2` |
| 40659 | Markers: Add/move marker 3 to play/edit cursor | `Ctrl+3` |
| 40660 | Markers: Add/move marker 4 to play/edit cursor | `Ctrl+4` |
| 40661 | Markers: Add/move marker 5 to play/edit cursor | `Ctrl+5` |
| 40662 | Markers: Add/move marker 6 to play/edit cursor | `Ctrl+6` |
| 40663 | Markers: Add/move marker 7 to play/edit cursor | `Ctrl+7` |
| 40664 | Markers: Add/move marker 8 to play/edit cursor | `Ctrl+8` |
| 40665 | Markers: Add/move marker 9 to play/edit cursor | `Ctrl+9` |

来源：[Linux 7.82 动作目录](linux-7.82/actions.tsv)。前三项基础定位/插入语义、40613 近光标删除与 40656–40665 编号 Marker 动作均已接入。

原厂 `Edit Marker` 对话框提供 ID、Lane、Position 和 Marker Manager 入口。Default 7 当前小节/拍显示下，Position 使用 `bar.beat.hundredths`；将 Marker 从 `3.1.50` 改到 `3.2.00` 会移动 Marker，编辑光标保持在 `3.1.50`。改成 `3.2.00` 后按 Cancel，Manager 的 Start 仍为 `3.1.50`。字段截图、提交后的时间线、取消结果和逐步观察见[原厂属性窗](fixtures/marker-edit-position-native.png)、[光标与 Marker 分离](fixtures/marker-edit-position-cursor-native.png)、[取消结果](fixtures/marker-edit-position-cancel-native.png)和[Position TSV](fixtures/marker-edit-position-native.tsv)。扩展原厂探针后确认，ID 是可编辑的正整数 Marker 编号；编号可稀疏，也允许重复。编号动作按可见编号查找，重复时移动位置最早的匹配项；缺号时新增该编号。普通 Insert 选择最小可用正编号。探针结果见[编号身份 TSV](fixtures/project-marker-id-slots-native.tsv)。Lane 字段的可编辑性/保存语义还未确认。这轮没有独立证明 Position 修改的原厂 Undo 语义，AAADAW 的位置 Undo 由项目历史测试验证，不据此宣称与原厂撤销粒度完全一致。

原厂实窗交互在同一 Linux 7.82 / Default 7 一次性工程中复核：单击 Marker 旗标/名称会将编辑光标定位到 Marker；拖动旗标会移动 Marker，拖动操作一次 Undo 即恢复；Snap/Grid Settings 默认启用 Snap，Arrange 网格为 `1/4`；“Use the same grid division in arrange view and MIDI editor” 默认未勾选，说明两者使用独立分度，但这次标记探针未打开 MIDI Editor 测量其具体默认值。吸附开启时拖动落到四分音符网格，关闭时可放置在网格线之间。原厂工程位置、界面结果和 undo 观察见[鼠标交互 TSV](fixtures/marker-ruler-native.tsv)；[标记单击实窗](fixtures/marker-ruler-select-native.png)与[Snap/Grid Settings](fixtures/marker-ruler-snap-grid-native.png)截图存档。

Linux REAPER 7.82 Default 7 的 `View: Show region/marker manager window` 是 action `40326`，默认键 `Ctrl+Alt+Shift+R`。原厂窗口标题为 `Region/Marker Manager`，本次无窗口管理器下的客户区为 `674×387`；初始过滤状态是 Regions 勾选、Markers 未勾选。勾选 Markers 后，表格显示 `#`、Name、Start、End、Length、Current、Visible、Ruler Lane 等列。Marker 行单击会选中该行、将编辑光标移到标记并滚动 Arrange 视口；在停止状态下此操作不改工程内容。双击表格单元进入单元格内编辑，撤销可恢复 Marker ID。一次性工程包含空名称 Marker 1（4 秒）和 Marker 10（11 秒）；默认窗口、勾选 Markers 后的列表和第二行选择后光标移动见[原厂 Manager 截图](fixtures/marker-manager-default-native.png)、[Marker 列表](fixtures/marker-manager-markers-native.png)、[选择/定位](fixtures/marker-manager-selection-native.png)，逐步观察见[原厂 Manager TSV](fixtures/marker-manager-native.tsv)。

## 已实现

- Project 用内部 `MarkerId` 维持稳定身份，并单独保存可编辑的 REAPER Marker number；编号可以稀疏或重复。同位置 Marker 按内部稳定 ID 排序，空名称显示可见编号。
- Marker 的插入、属性更新、移动和删除均属于 `DawAction`，支持事件撤销/重做和快照恢复；SQLite schema 36 保存可见编号，schema 35 迁移保留原有稳定 ID/名称/位置并从 ID 还原旧版显示编号，schema 34 工程迁移后为空集合。
- Insert 菜单和原厂默认 `M` 在编辑光标处添加 Marker；`[` 定位上一个 Marker，找不到时回到项目起点；`]` 定位下一个 Marker，找不到时定位到可推导的 Item 内容末尾。
- Arrange 标尺显示 Marker 旗帜、贯穿标尺的竖线和名称；Marker 竖线延伸到全部时间线轨道，未命名 Marker 使用序号。标尺坐标差使用有符号整数运算，避免视口位于 Marker 右侧时发生无符号回绕。
- 双击 Marker 旗标/标签打开 `Edit Marker`；名称输入在打开时获得焦点。Position 字段格式化/解析 Default 7 的 `bar.beat.hundredths`，可编辑 Marker number 的范围是 1 至 2,147,483,647；Enter 或 OK 将 Name、ID 与 Position 合并为一次 `UpdateProjectMarkerProperties` 历史动作。Cancel、Escape 和关闭窗口会丢弃三项草稿，Undo/Redo 一步恢复全部属性并保留内部 MarkerId。Lane 明确标出尚不支持，颜色控件未实现。位置字段的负时间、其它原厂时间显示模式、位置舍入边界与原厂 Undo 粒度仍未对齐。
- View 菜单、默认键 `Ctrl+Alt+Shift+R` 和稳定 action id `view.marker-manager` 可打开独立 `Region/Marker Manager` 窗口。窗口保留原厂初始过滤状态（Regions 开启、Markers 关闭）；开启 Markers 后，表格按 `#`、Name、Start、Current 列出 Marker，筛选名称、可见编号或 `Bar.Beat.hundredths`，单击行移动编辑光标，Edit 使用现有属性窗，Delete 走稳定 MarkerId 的 `DawAction`，支持 Undo/Redo。空名称 Marker 的显示编号使用可见 Marker number；项目切换时清除行选择。删除控件在工程/播放忙碌时禁用，并且消息处理仍有忙碌守卫。
- Project Region 已有独立基础数据模型与 `RegionId`、可见编号、Start/End、Name；Time Selection 可通过 `Shift+R` / action `40174` 插入 Region，Manager 显示 Region 行并可筛选、选择、编辑 Name/ID/Start/End、删除及 Undo。schema 37 保存 Region，schema 36 迁移为空集合。当前自动编号、边界、Manager 编辑方式和 Region 绘制均未与原厂行为/像素核对；完整差异详见[Region 验证记录](project-regions.md)。

AAADAW 实窗的默认过滤/空状态、Marker 列表和编辑对话框分别见[默认窗口](fixtures/marker-manager-default-aaadaw.png)、[编辑后列表](fixtures/marker-manager-list-aaadaw.png)和[名称编辑窗](fixtures/marker-manager-edit-dialog-aaadaw.png)。实窗将名称改为 `verse` 后列表即时刷新，再从管理器按 Ctrl+Z 恢复空名称；针对管理器窗口的全局 Undo 路由也有回归测试。对照原厂截图可见当前布局和列仍只是基础实现，不能据此视为视觉或交互等价。
- Action List 中的“Delete project marker near cursor”按当前 tempo map 将 Marker 与编辑光标转换成项目采样位置；删除光标前后严格小于 1 秒的最近 Marker，光标正重合时距离为零并可删除，正好 1 秒则不删除。等距时原厂删除较早位置的 Marker；Region 不参与候选。删除使用稳定 ID 的 `DawAction`，所以撤销可恢复相同 Marker。
- `Ctrl+1`…`Ctrl+9` 与 `Ctrl+0` 对应 1…10 号 Add/move Marker 动作。编号查找使用可编辑的 REAPER Marker number，不依赖位置排序、名称或 Region；目标编号存在时移动它并保留名称/稳定 MarkerId，不存在时在光标创建指定编号的空名称 Marker。重复编号按位置优先选择最早的 Marker。普通 Insert 使用最小可用正编号。当前 AAADAW 只在停止播放时应用于编辑光标；播放中的 play cursor 动作保持禁用，等待实时安全与原厂行为验证。
- 标尺中的 Marker 旗标/名称可单击选择并将编辑光标定位到该 Marker；拖动会显示临时位置，释放时只提交一个稳定 ID 的 `DawAction::UpdateProjectMarker`，支持 Undo/Redo。拖动复用编排当前 Snap 开关与网格；Arrange 默认网格为原厂实测的 `1/4`，MIDI Editor 有独立网格分度控制，AAADAW 当前默认 `1/16`（该具体数值尚未在原厂 MIDI Editor 实窗复核），并与 Arrange 共用 Snap 开关。项目编辑忙碌或播放期间会拒绝最终提交，不在实时播放图上写入工程状态。

实现位置：[core Marker 模型](../../../crates/aaadaw-core/src/markers.rs)、[Project actions 与历史](../../../crates/aaadaw-core/src/project.rs)、[SQLite 持久化](../../../crates/aaadaw-storage/src/store.rs)、[Marker 命令](../../../crates/aaadaw/src/app/commands.rs)、[标尺绘制](../../../crates/aaadaw/src/timeline.rs)。探针脚本：[近光标删除](../../../scripts/reaper_parity/probe-marker-delete.lua)、[编号 Marker 基础动作](../../../scripts/reaper_parity/probe-numbered-markers.lua)、[编号与重复身份](../../../scripts/reaper_parity/probe-project-marker-id-slots.lua)；原厂输出：[近光标删除 TSV](fixtures/marker-delete-near-cursor-native.tsv)、[编号 Marker TSV](fixtures/numbered-markers-native.tsv)、[编号身份 TSV](fixtures/project-marker-id-slots-native.tsv)。

## 验证

- `project_markers` core 集成测试：稳定 ID、按位置排序、更新、快照恢复及 Undo/Redo；无效输入不会污染项目历史。
- `project_markers_round_trip_and_schema_34_migrates_without_markers`、`schema_35_marker_rows_migrate_to_visible_numbers_without_losing_stable_ids`、`project_marker_round_trip_preserves_sparse_and_duplicate_numbers`：schema 36 保存/加载、从 schema 34 创建表、从 schema 35 恢复编号字段，以及稀疏/重复编号往返。
- `project_markers_insert_navigate_and_undo_through_app_actions`：应用插入、前后定位、起点回退及 Undo/Redo。
- `marker_actions_use_reaper_default_keys_and_stable_action_ids`：`M`、`[`、`]` 与稳定动作 ID。
- `numbered_marker_actions_use_reaper_slots_and_default_shortcuts`：编号 1–10 的稳定动作 ID 和 `Ctrl+1`…`Ctrl+0` 默认键。
- `numbered_marker_slots_match_visible_numbers_and_duplicate_order`：按 REAPER 可见编号移动、重复编号优先选择最早位置、缺号创建目标编号并 Undo。
- `visible_numbers_are_independent_from_stable_ids_and_may_be_sparse_or_duplicated`、`plain_marker_insertion_uses_lowest_available_positive_number`：稀疏/重复可见编号与稳定 ID 分离、属性编辑原子 Undo/Redo 和普通插入补最小编号。
- `ruler_marker_click_selects_marker_and_moves_edit_cursor`、`ruler_marker_drag_preserves_grab_offset_and_publishes_one_move`、`ruler_marker_drag_uses_enabled_quarter_note_snap_grid`：标尺点击/拖动命中、鼠标抓取偏移、Snap 网格预览与单次提交。
- `ruler_marker_double_click_opens_marker_editor_without_second_selection`、`second_marker_click_can_become_a_drag_instead_of_opening_the_editor`：双击入口和第二击转为拖动的手势保护。
- `marker_properties_edit_name_number_and_position_as_one_undoable_change`、`marker_properties_keep_invalid_or_busy_edits_open_without_changing_project`：Cancel 不写入、Name/ID/bar.beat.hundredths Position 一次提交、位置 ticks 映射、内部身份保留、Undo/Redo，以及非法输入/忙碌保护。Linux 实窗截图见[AAADAW 编辑窗](fixtures/marker-edit-position-aaadaw.png)、[位置已提交](fixtures/marker-edit-position-applied-aaadaw.png)和[位置撤销](fixtures/marker-edit-position-undo-aaadaw.png)。
- `marker_manager_filters_selects_edits_and_deletes_with_undo`、`marker_manager_searches_name_id_and_reaper_bar_beat_position`：管理器默认过滤状态、筛选数据、选择定位、编辑入口、删除/Undo 与 REAPER bar.beat 格式。
- `ruler_marker_move_is_one_undoable_edit_and_respects_busy_guard`：移动保留 Marker 身份/名称，单步 Undo/Redo，工程忙碌时拒绝修改。
- `arrange_and_midi_snap_grids_match_defaults_and_change_independently`、`piano_roll_paste_uses_its_own_grid_division_and_respects_shared_snap_off`：Arrange/MIDI Editor 的网格间隔独立，Snap 开关共用；原厂 MIDI Editor 的初始分度仍待实窗测量。
- `marker_ruler_positions_use_signed_tick_distance`：视口前后坐标均使用正确的有符号距离；`timeline_shader_receives_project_markers_for_full_height_lines` 确认标记 ticks 被送入整轨高度的画布绘制。
- `deleting_nearby_marker_matches_reaper_radius_and_nearest_selection`：验证两侧最近 Marker、严格小于 1 秒、恰好 1 秒与重合位置边界、等距时较早 Marker 优先、稳定 ID 与 Undo。原厂探针另验证等距 tie-break，且 Region 不参与候选。

## 尚未对齐

这只是基础 Marker/Manager/Region 切片，不代表 Marker/Region parity 完成。Manager 仍未实现原厂 End、Visible、Ruler Lane、Render Matrix、Lane Manager、Options 列/功能、列拖动和单元格内编辑；颜色/隐藏、Lane 字段语义、完整 Manager 布局以及 Region 原厂行内编辑方式尚待确认。Region 的自动编号、同位置排序、边界 hit testing/拖动、吸附、负时间、循环联动、Render Matrix、tempo/timebase 变化及原生保存语义仍未验收。Edit Marker 的 Name、Marker number 与默认 `bar.beat.hundredths` Position 已可编辑；Position 的负时间、其它时间显示模式、舍入边界及原厂 Undo 粒度仍待对照。尚未实现或实测：同位置 Marker 的独立选择/逐个导航/删除优先级，播放中的 play cursor 编号 Marker 动作，颜色与隐藏状态，marker timebase，以及 tempo/meter 变化时原厂位置保持语义。不同鼠标修饰键、同位置重叠旗标和原厂/AAADAW 全主题视觉仍需测量。Snap Grid 目前只有 REAPER 默认 `1/4` 与基础交互已对照，完整网格设置树和各网格边界未封闭。`[`/`]` 的边界行为和时间尺外观还需在原厂及 AAADAW 实窗逐项比较；目前项目末尾按现有 Item 内容推导，不能代表所有 REAPER 工程结束规则。不要把这些基础切片计为 UI-013、P4 或完整复刻已完成。
