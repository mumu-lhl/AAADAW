# Loop points and Time Selection — Linux REAPER 7.82

## 原厂行为

在 Linux REAPER 7.82、Default 7、空工程与 Dummy Audio 下，通过 Main action section 和 ReaScript 验证 loop/time 两种范围及其独立链接状态。默认链接为开启；启用 40621 会把当前 Time Selection 复制到 Loop points，关闭链接保留两者当前范围。未链接时 40622 将 Time Selection 复制到 Loop points，40623 将 Loop points 复制到 Time Selection。

清除行为有意不同：40624 只在未链接时清除 Loop points，链接时无操作；40634 清除 Loop points，若链接则两种范围都被清除；40635 对 Time Selection 同理；40020 无论链接状态都清除两种范围，默认绑定 Escape。逐用例原始结果见[原厂 TSV](fixtures/loop-time-selection-linkage-native.tsv)，可复现脚本见[探针](../../../scripts/reaper_parity/probe-loop-time-selection-linkage.lua)。

默认 Main action section 还将 `{`/`}` 分配给 40222/40223。无 Loop points 时，40222 把 Start 和 End 都设为编辑光标位置；40223 建立从工程零点到编辑光标的范围。已有范围内设置端点会保留另一端；若新端点越过另一端，原厂把区间折叠到新端点。链接时端点变化同步到 Time Selection，未链接时保持 Time Selection 不变。原始用例见[端点编辑 TSV](fixtures/loop-point-edit-native.tsv)，复现脚本见[端点探针](../../../scripts/reaper_parity/probe-loop-point-edit.lua)。

未链接时的原厂画面见[Arrange 截图](fixtures/loop-time-selection-unlinked-native.png)：Time Selection 仍以 Arrange 区域色带显示；独立 Loop points 在标尺下方以窄横向括线和两端手柄显示，不在轨道区画两条全高边线或给循环区间着色。

用原厂鼠标在底栏拖动单独端点：未链接时把 Loop start 从 3 秒拖到 2.5 秒，只改变 Loop points，Time Selection 仍为 `[1,2]`；重新建立基线后把 Loop end 从 4 秒拖到 4.5 秒，同样只改变 Loop points。逐阶段结果和可复现的手动探针见[端点拖动 TSV](fixtures/loop-point-drag-native.tsv)与[拖动探针](../../../scripts/reaper_parity/probe-loop-point-drag.lua)。

底栏手势探针进一步确认：普通点击空白 lane 只移动编辑光标；普通拖动已有 bracket 主体或空白 lane 都把 loop 折叠成鼠标按下处的零长度点。Shift+拖动 bracket 主体移动整个区间并保留长度，未链接时 Time Selection 不变；Snap 开启时按 `1/4` 网格移动。Shift+拖动空白 lane 仍只创建零长度点；链接时 Time Selection 同步到该点。各阶段的范围、游标、Snap 与链接值见[底栏手势 TSV](fixtures/loop-point-ruler-gestures-native.tsv)，[可复现探针](../../../scripts/reaper_parity/probe-loop-point-ruler-gestures.lua)按脚本中的阶段名等待手动输入。

工程保存探针在 REAPER 7.82 用 `Main_SaveProjectEx` 写出两个 disposable `.rpp`，再从独立 REAPER 实例打开文件并通过 `GetSet_LoopTimeRange2` 回读。未链接用例恢复 Time Selection `[1.25, 2.5]` 与 Loop `[3.5, 5.75]`；linked 用例保存时两者均为 `[1.25, 2.5]`，重开后同样恢复。RPP 分别以 `SELECTION` 和 `SELECTION2` 存这两个秒数范围。40621 链接开关没有写入该工程文件；重开后反映隔离配置中的全局 `locklooptotime` 状态。逐阶段数据见[持久化 TSV](fixtures/loop-time-selection-persistence-native.tsv)，探针见[保存/重开脚本](../../../scripts/reaper_parity/probe-loop-persistence.lua)。

## AAADAW 当前实现

Timeline 维护单独的 `time_selection`、`loop_points` 与 `loop_points_linked` 状态。Loop points 允许零长度，以保留原厂在编辑光标越过另一端后的折叠结果。默认链接开启；通过现有选区手势更新 Time Selection 时，链接状态会同步 Loop points；设置端点或代码层设置 Loop points 时也按相同规则同步 Time Selection。Action List 加入 40222/40223、40621/40622/40623/40624/40634/40635、40020 对应操作，40222/40223 分别默认绑定 `{`/`}`，40020 的 Escape 行为通过全局取消路径清除两种范围。

Ruler 现在按 Default 7 底栏绘制窄括线与两端手柄；拖动任一端点时显示预览，释放时一次性提交。普通拖动底栏建立按下位置的零长度点；Shift+拖 bracket 主体会显示保留区间长度的预览，按当前 Snap 网格移动起点并一次提交，链接时同步 Time Selection。空白 lane 点击在释放时定位编辑光标，单击不创建范围。Time Selection 与 Loop points 现以相对工程采样原点的秒数随工程状态原子保存，重载后按该工程的 tempo map 还原到 timeline tick；时间范围编辑会标记工程未保存。40621 的链接偏好独立保存在平台全局配置目录下的 `loop-time-preferences.json`，默认开启，变更经 500 ms 防抖后原子写入，并在主窗口关闭前刷新；它不会标记工程为未保存。实窗截图[AAADAW Loop/Time Selection](fixtures/loop-time-selection-aaadaw.png)在未链接的 `[1,2]` Time Selection 与 `[3,4]` Loop points 下显示该底栏；它与原厂截图的窗口尺寸/缩放不同，不作为完整窗口几何验收。

这仍不构成完整 loop 功能：原厂空白 lane 拖动只建立零长度点，尚未找到在该 lane 一次创建非零范围的手势；Ctrl/Alt/Super、拖动中更换修饰键、端点及空白点创建的 Snap 开关语义、整段移动边界与 Action List 链接勾选状态仍待核验；当前只证明 Shift 整段移动的 Snap 行为，空白点按原始鼠标按下位置实现。Repeat 播放与 Transport 联动、Tempo/Meter/timebase 边界及完整 REAPER 原厂窗口视觉对照也未完成。播放引擎对媒体流只允许向前读取，不能据此声称 Repeat 已工作。

## 验证

`loop_points_and_time_selection_follow_reaper_link_copy_and_clear_actions` 覆盖链接/复制/清除动作；`ruler_loop_start_handle_previews_and_publishes_a_loop_endpoint_drag` 覆盖端点预览/提交；`ruler_shift_drag_moves_the_loop_body_and_preserves_its_length`、`ruler_shift_drag_snaps_the_loop_start_and_preserves_its_length`、`plain_drag_in_the_loop_body_sets_a_point_at_the_drag_origin`、`dragging_an_empty_bottom_ruler_lane_creates_a_point_at_the_drag_origin` 和 `clicking_an_empty_bottom_ruler_lane_sets_the_edit_cursor_without_creating_loop_points` 覆盖底栏手势。最终全 workspace 验证结果记录在 [`progress.md`](../../reaper-parity/progress.md)。该功能没有创建/修改实时音频数据路径。
