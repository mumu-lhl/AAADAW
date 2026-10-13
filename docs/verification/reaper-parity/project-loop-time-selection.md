# Loop points and Time Selection — Linux REAPER 7.82

## 原厂行为

在 Linux REAPER 7.82、Default 7、空工程与 Dummy Audio 下，通过 Main action section 和 ReaScript 验证 loop/time 两种范围及其独立链接状态。默认链接为开启；启用 40621 会把当前 Time Selection 复制到 Loop points，关闭链接保留两者当前范围。未链接时 40622 将 Time Selection 复制到 Loop points，40623 将 Loop points 复制到 Time Selection。

清除行为有意不同：40624 只在未链接时清除 Loop points，链接时无操作；40634 清除 Loop points，若链接则两种范围都被清除；40635 对 Time Selection 同理；40020 无论链接状态都清除两种范围，默认绑定 Escape。逐用例原始结果见[原厂 TSV](fixtures/loop-time-selection-linkage-native.tsv)，可复现脚本见[探针](../../../scripts/reaper_parity/probe-loop-time-selection-linkage.lua)。

默认 Main action section 还将 `{`/`}` 分配给 40222/40223。无 Loop points 时，40222 把 Start 和 End 都设为编辑光标位置；40223 建立从工程零点到编辑光标的范围。已有范围内设置端点会保留另一端；若新端点越过另一端，原厂把区间折叠到新端点。链接时端点变化同步到 Time Selection，未链接时保持 Time Selection 不变。原始用例见[端点编辑 TSV](fixtures/loop-point-edit-native.tsv)，复现脚本见[端点探针](../../../scripts/reaper_parity/probe-loop-point-edit.lua)。

未链接时的原厂画面见[Arrange 截图](fixtures/loop-time-selection-unlinked-native.png)：Time Selection 仍以 Arrange 区域色带显示；独立 Loop points 在标尺下方以窄横向括线和两端手柄显示，不在轨道区画两条全高边线或给循环区间着色。

## AAADAW 当前实现

Timeline 维护单独的 `time_selection`、`loop_points` 与 `loop_points_linked` 状态。Loop points 允许零长度，以保留原厂在编辑光标越过另一端后的折叠结果。默认链接开启；通过现有选区手势更新 Time Selection 时，链接状态会同步 Loop points；设置端点或代码层设置 Loop points 时也按相同规则同步 Time Selection。Action List 加入 40222/40223、40621/40622/40623/40624/40634/40635、40020 对应操作，40222/40223 分别默认绑定 `{`/`}`，40020 的 Escape 行为通过全局取消路径清除两种范围。

这只是状态与动作基础，尚不构成完整 loop 功能：当前没有独立 loop-point 标尺绘制/拖动、Action List 的链接勾选状态、播放循环、Repeat 与 Transport 联动、配置持久化、工程保存恢复、Ruler/Edit cursor/Tempo/Meter/timebase 边界测试或 REAPER 原厂窗口视觉对照。项目加载时按现有 Time Selection 生命周期清空两个范围；保存语义尚待单独对照。

## 验证

`loop_points_and_time_selection_follow_reaper_link_copy_and_clear_actions` 覆盖默认链接、链接建立/解除、解除后的独立编辑、双向复制、条件清除与链接清除、Escape 和端点设置，以及 Action List 稳定 ID。最终全 workspace 验证结果记录在 [`progress.md`](../../reaper-parity/progress.md)；本功能没有创建/修改实时音频数据路径。
