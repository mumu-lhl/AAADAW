# Project Regions — Linux REAPER 7.82

## 原厂参照

默认 Main section 的动作目录确认 `Markers: Insert region from time selection` 为 action `40174`，默认键为 `Shift+R`；`40306` 是插入并打开编辑窗，`40615`/`40616` 是光标附近删除/编辑 Region，`40326` 打开 Region/Marker Manager。来源是仓库已核对的 [Linux REAPER 7.82 动作目录](linux-7.82/actions.tsv)。

这轮没有新采集原厂 Region Manager 截图、字段输入、自动编号、范围边界拖动、循环点联动、Region Render Matrix、保存格式或播放中的行为。下文凡是只由 AAADAW 测试确认的细节都属于当前实现行为，不代表原厂行为。Region 可见编号重复规则、编号分配、最大合法 ID、同位置排序、包含/相邻边界和时间基准语义仍待逐项探针验证。

## 已实现的基础覆盖

- Core 为 Region 使用稳定内部 `RegionId`，并保存可编辑显示编号、起点/终点 ticks 和名称。插入、更新、删除属于 `DawAction`，Project 历史支持 Undo/Redo，快照加载验证并排序。
- SQLite schema 37 新增 `project_regions` 表；schema 36 工程迁移后得到空 Region 集合。Round-trip 保留 ID、编号、区间和名称。
- Insert 菜单动作与 `Shift+R` 只在非空 Time Selection 可用；从时间选择创建空名称 Region，默认从最小正编号开始分配。编辑/导入动作在播放、录音、I/O 等工程忙碌期间被拒绝。编号策略尚未原厂实测。
- Arrange 时间线显示 Region 区间底色与起止线；当前 Region 标尺手势已实现基础点击/移动/边界调整，具体命中区和视觉仍未与原厂实窗对照。
- Arrange 标尺的 Region 色带点击会选择整个区间并把编辑光标移至起点；色带内拖动会保持长度移动 Region，下方边线拖动会单独改 Start/End，并复用 Arrange Snap/Grid。拖动预览后释放提交一条稳定 RegionId 的更新；Undo/Redo 保留编号和名称。播放或工程忙碌期间不提交。
- Region/Marker Manager 的 Regions 默认过滤状态现在由真实 Region 数据驱动。Region 行显示 `R<number>`、名称、Start、End、Length 与光标是否位于区间内；可按名称、编号、起点或终点筛选。选中行设置 Time Selection 并将编辑光标移动至起点；Edit 窗口可草拟 Name、ID、Start、End，统一提交为一条可撤销更新；Delete 使用稳定 ID 并支持 Undo。
- Region 属性窗复用当前 Default 7 `bar.beat.hundredths` 位置格式。无效编号/位置、反向或零长度范围会保留窗口且不改工程；Cancel、Escape 或关闭窗口丢弃草稿。其它原厂时间显示模式和负位置还未支持。

## 验证

- `project_regions` core 集成测试覆盖稳定 ID、排序、插入/更新/删除、Undo/Redo、快照、非法边界和当前自动编号策略。
- `project_regions_round_trip_and_schema_36_migrates_without_regions` 覆盖 SQLite 往返以及 schema 36 → 37 空集合迁移。
- 应用回归覆盖无时间选择时禁用、反向选择归一后按区间插入、`Shift+R`/稳定 action ID、Manager 筛选/选择/编辑/删除/Undo、Region 属性拒绝错误输入和取消草稿。
- `ruler_region_click_selects_its_range_and_start_cursor`、`ruler_region_start_edge_drag_updates_one_boundary`、`ruler_region_end_edge_drag_updates_one_boundary`、`ruler_region_body_drag_moves_range_without_changing_length`、`ruler_region_edge_drag_snaps_to_arrange_grid` 覆盖标尺手势与预览；`ruler_region_edit_preserves_identity_and_is_one_undoable_action`、`ruler_region_edit_obeys_project_busy_guard` 和 `ruler_region_edit_rejects_a_stale_drag_after_another_edit` 覆盖单动作提交、忙碌守卫和并发草稿冲突保护。
- `timeline_shader_receives_project_markers_for_full_height_lines` 同时确认 Region 区间被交给 GPU Arrange primitive；这验证数据接线，不构成原厂像素或色彩验收。

最终代码验证：`xvfb-run -a rtk cargo nextest run --workspace --no-fail-fast -j 1` 为 950/950 通过、0 跳过（66 个测试二进制）；workspace all-target Clippy `-D warnings`、`cargo check --workspace --all-targets`、`cargo build -p aaadaw`、fmt 和 `git diff --check` 通过。Region 不改变实时音频 callback 的数据结构或同步方式，项目编辑仍受现有 playback/recording busy guard 约束。

## 尚未对齐

Region 的完整原厂行为仍未完成：本次标尺点击/移动/边界拖动与 `1/4` 默认网格仅由 AAADAW 手势测试覆盖，鼠标区域、阈值、Snap 修饰键、重叠边界命中和 preview/Undo 尚未在原厂实窗对照；Region Manager 的原厂行内编辑、列可见性/排序/拖动、颜色与隐藏、Ruler Lane、管理器选区范围、边界吸附、ID 分配与重编号、负时间和所有时间格式、循环/Repeat 行为、播放中操作、Tempo/Meter 变化下的位置保持、原生工程格式导入/导出、Region Render Matrix 和完整 Default 7 窗口几何/视觉对照也仍未完成。矩阵 `PROJECT-REGIONS-001`、`PROJECT-MARKERS-001`、UI-013 与 P4 保持 `in_progress`；本文件只记录实现范围和验证边界。
