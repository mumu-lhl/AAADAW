# Project Regions — Linux REAPER 7.82

## 原厂参照

默认 Main section 的动作目录确认 `Markers: Insert region from time selection` 为 action `40174`，默认键为 `Shift+R`；`40306` 是插入并打开编辑窗，`40615`/`40616` 是光标附近删除/编辑 Region，`40326` 打开 Region/Marker Manager。来源是仓库已核对的 [Linux REAPER 7.82 动作目录](linux-7.82/actions.tsv)。

本次在隔离的 Linux REAPER 7.82、Default 7、空工程中探测了 `40615`/`40616`。Delete 使用编辑光标而非鼠标位置，只处理光标所在的 Region；区间为 `[Start, End)`，空隙不匹配，Marker 不参与，活动 Time Selection 不改变目标。重叠时选择 Start 最晚的 Region；Start 相同则选较短区间；边界完全相同则选择较早枚举的 Region。完整用例见 [原厂删除探针](fixtures/region-delete-near-cursor-native.tsv)。另实测原厂动作 `43486`（所选 Region 可见性）和 `43489`（Regions/Markers 标尺组合可见性），证据及未验证的组合选择语义见[可见性原厂记录](fixtures/region-visibility-native.tsv)。

`40616` 打开原厂 Edit Region 对话框且不移动编辑光标。默认对话框包含 Name、ID、Lane、Position、Length，以及 Region Manager、Region Render Matrix、Set color、Reset color、OK、Cancel；位置/长度使用 Bars.Beats 显示。原厂截图与观察记录见[原厂编辑窗口](fixtures/region-edit-dialog-native.png)和[编辑动作探针](fixtures/region-edit-near-cursor-native.tsv)。本次仍未探测原厂 Region Manager 全部列/交互、自动编号、范围边界拖动、循环点联动、保存格式或播放中的行为；编号重复规则、编号分配、最大合法 ID、同位置排序及时间基准语义也仍待逐项验证。

## 已实现的基础覆盖

- Core 为 Region 使用稳定内部 `RegionId`，并保存可编辑显示编号、起点/终点 ticks 和名称。插入、更新、删除属于 `DawAction`，Project 历史支持 Undo/Redo，快照加载验证并排序。
- SQLite schema 37 新增 `project_regions` 表；schema 36 工程迁移后得到空 Region 集合。schema 48 新增单个 Region 与整个 Region 标尺的独立可见性，旧工程默认可见；Round-trip 保留 ID、编号、区间、名称与可见状态。
- Insert 菜单动作与 `Shift+R` 只在非空 Time Selection 可用；从时间选择创建空名称 Region，默认从最小正编号开始分配。编辑/导入动作在播放、录音、I/O 等工程忙碌期间被拒绝。编号策略尚未原厂实测。
- Arrange 时间线显示 Region 区间底色与起止线；当前 Region 标尺手势已实现基础点击/移动/边界调整，具体命中区和视觉仍未与原厂实窗对照。
- Arrange 标尺的 Region 色带点击会选择整个区间并把编辑光标移至起点；色带内拖动会保持长度移动 Region，下方边线拖动会单独改 Start/End，并复用 Arrange Snap/Grid。拖动预览后释放提交一条稳定 RegionId 的更新；Undo/Redo 保留编号和名称。播放或工程忙碌期间不提交。
- Region/Marker Manager 的 Regions 默认过滤状态现在由真实 Region 数据驱动。Region 行显示 `R<number>`、名称、Start、End、Length、Visible 与光标是否位于区间内；可按名称、编号、起点或终点筛选。选中行设置 Time Selection 并将编辑光标移动至起点；Edit 窗口可草拟 Name、ID、Start、End，统一提交为一条可撤销更新；Delete 使用稳定 ID 并支持 Undo。
- Region 属性窗复用当前 Default 7 `bar.beat.hundredths` 位置格式。无效编号/位置、反向或零长度范围会保留窗口且不改工程；Cancel、Escape 或关闭窗口丢弃草稿。其它原厂时间显示模式和负位置还未支持。
- 原厂 `40615`/`40616` 已有对应的 “Delete region near cursor” / “Edit region near cursor” 命令。Delete 使用实测包含边界与重叠优先级，复用稳定 RegionId 的 `DawAction`，支持 Undo 且受工程忙碌保护；Edit 按当前光标目标打开 AAADAW 属性窗。这个 AAADAW 窗口仍只有 Name、ID、Start、End，缺少原厂 Lane、独立 Length、颜色操作和 Region Manager/Render Matrix 按钮，不能视为原厂对话框复刻完成。
- Manager 的 Region `Visible` 列、View 菜单 Region 标尺开关及 Action List 的所选/全部 Region 与 Regions+Markers 动作已经接入。单个 Region 可见性和整个 Region 标尺状态分别进入撤销、快照及 SQLite；隐藏项目不会绘制或响应时间线命中。原厂 `43486` 与 AAADAW 所选 Region 动作均在实际窗口中执行，并验证单步 Undo/Redo；AAADAW 菜单、Manager、Action List、Arrange 与历史状态截图见[运行清单](fixtures/region-visibility-aaadaw.tsv)。原厂 `43488` 用混合选区（R1 隐藏、M1 可见）实测为统一显示所有选中项；单步 Undo 恢复混合状态，Main action `40030` Redo 再次显示所有选中项。AAADAW 同一混合选区的 Action List 执行、Undo、Redo 也已实窗验证，见[组合动作运行记录](fixtures/region-visibility-selected-combined-aaadaw.tsv)。全局组合动作 `43489` 的原厂探针观察到：若 Region 或 Marker 标尺任一隐藏则一次运行显示两者；两者均显示时一次运行隐藏两者。AAADAW 的全部 Regions+Markers 标尺动作仍待实窗验证，App 测试套件也未完成链接。

## 验证

- `project_regions` core 集成测试覆盖稳定 ID、排序、插入/更新/删除、Undo/Redo、快照、非法边界和当前自动编号策略。
- `region_cursor_target_matches_native_half_open_and_overlap_rules` 覆盖原厂探测出的半开区间、重叠 Start 优先级、相同 Start 的较短范围优先级和完全相同范围的稳定顺序；`region_cursor_actions_edit_delete_and_undo_with_project_busy_guard` 覆盖动作注册、编辑目标、删除 Undo、Marker 隔离和忙碌拒绝。
- `project_regions_round_trip_and_schema_36_migrates_without_regions` 覆盖 SQLite 往返以及 schema 36 → 37 空集合迁移。
- 应用回归覆盖无时间选择时禁用、反向选择归一后按区间插入、`Shift+R`/稳定 action ID、Manager 筛选/选择/编辑/删除/Undo、Region 属性拒绝错误输入和取消草稿。
- `ruler_region_click_selects_its_range_and_start_cursor`、`ruler_region_start_edge_drag_updates_one_boundary`、`ruler_region_end_edge_drag_updates_one_boundary`、`ruler_region_body_drag_moves_range_without_changing_length`、`ruler_region_edge_drag_snaps_to_arrange_grid` 覆盖标尺手势与预览；`ruler_region_edit_preserves_identity_and_is_one_undoable_action`、`ruler_region_edit_obeys_project_busy_guard` 和 `ruler_region_edit_rejects_a_stale_drag_after_another_edit` 覆盖单动作提交、忙碌守卫和并发草稿冲突保护。
- `timeline_shader_receives_project_markers_for_full_height_lines` 同时确认 Region 区间被交给 GPU Arrange primitive；这验证数据接线，不构成原厂像素或色彩验收。
- `region_and_ruler_visibility_are_undoable_and_survive_snapshots` 验证领域动作、Undo/Redo 与快照；Storage 回归验证 schema 47→48 默认可见迁移及 Region/标尺状态往返。应用测试已通过类型检查和 Clippy；App 测试链接曾启动但因空间不足中止，未运行套件，因此以新构建桌面版的真实窗口交互补充验证。
- `selected_region_visibility_actions_batch_selection_with_one_undo_and_redo` 源码回归覆盖混合 Region/Marker 集合统一显示、单步 Undo/Redo 与动作注册；测试源码已通过类型检查和 Clippy，App 测试 suite 因链接磁盘空间不足未运行。原厂 action 43488 的实际混合选区、Undo/Redo 实窗证据见[原厂记录](fixtures/region-visibility-native.tsv)。

最终代码验证：`DISPLAY=:101 rtk cargo nextest run --workspace --no-fail-fast -j 1` 为 952/952 通过、0 跳过（66 个测试二进制）；workspace all-target Clippy `-D warnings`、`cargo check --workspace --all-targets`、`cargo build -p aaadaw`、fmt 和 `git diff --check` 通过。Region 不改变实时音频 callback 的数据结构或同步方式，项目编辑仍受现有 playback/recording busy guard 约束。

## 尚未对齐

Region 的完整原厂行为仍未完成：本次验证了 40615/40616 的目标选择与 Edit Region 对话框组成，但 AAADAW 目前只映射动作语义，尚未复刻该原厂窗口及其字段/按钮。标尺点击/移动/边界拖动与 `1/4` 默认网格仅由 AAADAW 手势测试覆盖，鼠标区域、阈值、Snap 修饰键、重叠边界命中和 preview/Undo 尚未在原厂实窗对照；Region Manager 的行内编辑、列可见性/排序/拖动、颜色、Ruler Lane、管理器选区范围、边界吸附、ID 分配与重编号、负时间和所有时间格式、循环/Repeat 行为、播放中操作、Tempo/Meter 变化下的位置保持、原生工程格式导入/导出及完整 Default 7 窗口几何/视觉对照也仍未完成。AAADAW 的全部 Regions+Markers 标尺动作尚未实窗验证；App 测试套件未能链接完成或运行。矩阵 `PROJECT-REGIONS-001`、`PROJECT-MARKERS-001`、UI-013 与 P4 保持 `in_progress`；本文件只记录实现范围和验证边界。
