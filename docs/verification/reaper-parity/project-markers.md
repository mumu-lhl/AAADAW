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

## 已实现

- Project 持有稳定 ID 的 Marker，位置以项目 ticks 保存；同位置 Marker 按稳定 ID 排序，空名称显示为当前序号。
- Marker 的插入、更新、删除均属于 `DawAction`，支持事件撤销/重做和快照恢复；SQLite schema 35 保存 Marker，schema 34 工程迁移后为空集合。
- Insert 菜单和原厂默认 `M` 在编辑光标处添加 Marker；`[` 定位上一个 Marker，找不到时回到项目起点；`]` 定位下一个 Marker，找不到时定位到可推导的 Item 内容末尾。
- Arrange 标尺显示 Marker 旗帜、贯穿标尺的竖线和名称；Marker 竖线延伸到全部时间线轨道，未命名 Marker 使用序号。标尺坐标差使用有符号整数运算，避免视口位于 Marker 右侧时发生无符号回绕。
- Action List 中的“Delete project marker near cursor”按当前 tempo map 将 Marker 与编辑光标转换成项目采样位置；删除光标前后严格小于 1 秒的最近 Marker，光标正重合时距离为零并可删除，正好 1 秒则不删除。等距时原厂删除较早位置的 Marker；Region 不参与候选。删除使用稳定 ID 的 `DawAction`，所以撤销可恢复相同 Marker。
- `Ctrl+1`…`Ctrl+9` 与 `Ctrl+0` 对应 1…10 号 Add/move Marker 动作。编号指按位置排序的第 N 个 Marker，不取名称，也不计 Region；N 号 Marker 存在时移动它并保留名称/稳定 ID，不存在时在光标插入空名称 Marker。移动后会按新位置重新排序。当前 AAADAW 只在停止播放时应用于编辑光标；播放中的 play cursor 动作保持禁用，等待实时安全与原厂行为验证。

实现位置：[core Marker 模型](../../../crates/aaadaw-core/src/markers.rs)、[Project actions 与历史](../../../crates/aaadaw-core/src/project.rs)、[SQLite 持久化](../../../crates/aaadaw-storage/src/store.rs)、[Marker 命令](../../../crates/aaadaw/src/app/commands.rs)、[标尺绘制](../../../crates/aaadaw/src/timeline.rs)。探针脚本：[近光标删除](../../../scripts/reaper_parity/probe-marker-delete.lua)、[编号 Marker](../../../scripts/reaper_parity/probe-numbered-markers.lua)；原厂输出：[近光标删除 TSV](fixtures/marker-delete-near-cursor-native.tsv)、[编号 Marker TSV](fixtures/numbered-markers-native.tsv)。

## 验证

- `project_markers` core 集成测试：稳定 ID、按位置排序、更新、快照恢复及 Undo/Redo；无效输入不会污染项目历史。
- `project_markers_round_trip_and_schema_34_migrates_without_markers`：schema 35 保存/加载和 schema 34 迁移。
- `project_markers_insert_navigate_and_undo_through_app_actions`：应用插入、前后定位、起点回退及 Undo/Redo。
- `marker_actions_use_reaper_default_keys_and_stable_action_ids`：`M`、`[`、`]` 与稳定动作 ID。
- `numbered_marker_actions_use_reaper_slots_and_default_shortcuts`：编号 1–10 的稳定动作 ID 和 `Ctrl+1`…`Ctrl+0` 默认键。
- `numbered_marker_slots_insert_move_by_position_and_undo`：按 Marker 排序序号创建/移动、保留自定义名称/稳定 ID，移动排序与 Undo。
- `marker_ruler_positions_use_signed_tick_distance`：视口前后坐标均使用正确的有符号距离；`timeline_shader_receives_project_markers_for_full_height_lines` 确认标记 ticks 被送入整轨高度的画布绘制。
- `deleting_nearby_marker_matches_reaper_radius_and_nearest_selection`：验证两侧最近 Marker、严格小于 1 秒、恰好 1 秒与重合位置边界、等距时较早 Marker 优先、稳定 ID 与 Undo。原厂探针另验证等距 tie-break，且 Region 不参与候选。

## 尚未对齐

这只是基础 Marker 切片，不代表 Marker/Region parity 完成。尚未实现或实测：Marker 选择、鼠标拖动、重命名/属性窗口、Marker 管理器、同位置 Marker 的独立选择/逐个导航/删除优先级、播放中的 play cursor 编号 Marker 动作、颜色与隐藏状态、区域创建/编辑/循环联动、负时间显示、marker timebase，以及 tempo/meter 变化时原厂位置保持语义。`[`/`]` 的边界行为和时间尺外观还需在原厂及 AAADAW 实窗逐项比较；目前项目末尾按现有 Item 内容推导，不能代表所有 REAPER 工程结束规则。不要把该切片计为 UI-013、P4 或完整复刻已完成。
