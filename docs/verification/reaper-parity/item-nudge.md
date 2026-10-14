# Item > Nudge/set items...

本记录覆盖 Linux REAPER `7.82/linux-x86_64`、Default 7 主题下 Main action 41226 `Item edit: Nudge/Set...`，以及 AAADAW 当前实现。目标是逐个选项、逐种编辑模式和快捷键行为继续对齐；本文列出的部分行为仍未达到原厂等价，矩阵状态保持 `in_progress`。

## 原厂参考

默认窗口为 528×121。初始状态是 `Nudge`、目标 `position`、`by: 1 milliseconds`、`Snap to unit` 关闭。Target 下拉依次列出 `position`、`left trim`、`left edge`、`end position`、`right trim`、`contents`、`duplicate`、`edit cursor`。Item 菜单在 `Select all` 后显示 `Nudge/set items...` 和 `N`；对应 Main action 是 41226。实际默认菜单截图还能看到其余未实现条目，因此它同时作为 Item 菜单尚未完整的证据。

原厂操作探针用 action 41249 `Item edit: Nudge right by last nudge dialog settings` 验证默认位置 Nudge：选中的 10 秒音频项从 0 移至 1 ms、长度和源偏移不变；选中的 5 秒 MIDI 项也从 0 移至 1 ms、长度不变；没有选中项时素材保持不变。三种情况都保留已有选择。REAPER 的 state-change 计数增量为 0，Undo 栈仍显示 `Insert media items`，没有独立的 Nudge 撤销记录。完整实测值见 [原厂 TSV](fixtures/item-nudge-native.tsv)。

## AAADAW 当前覆盖

AAADAW 的 Action List 入口 `Item edit: Nudge/Set...` 与菜单短标签 `Nudge/set items...` 现在共用一个动作 ID；菜单默认快捷键显示 `N`。Nudge/Set 子窗口为 528×121，Mode、Target、数值和 Unit 按原厂顺序排列，两个 target 菜单截图合起来显示完整八项。

已实现的编辑路径：

- Position / Nudge：同时移动所选 Audio 与 MIDI 项，保持长度、源偏移和 MIDI 内容；多项用一个共享 delta，间距不变，并作为一个可撤销工程事务。
- Position / Set：支持绝对位置和相对位移；多选时以最早的所选项为锚点，对其它项应用同一位移。
- Edit cursor：支持 Nudge 和绝对/相对 Set，不创建工程素材事务。
- Audio / Left Trim、Left Edge、End Position、Right Trim：按原厂 250 ms 探针调整对应边界；Left Trim 按素材源采样率换算并移动源偏移。若选择中包含 MIDI、时间越界或素材源采样率不可用，则整批拒绝。
- Duplicate：按 Nudge 量复制所选 Audio 和 MIDI 项；副本成为唯一选择，并通过一个工程事务支持 Undo。
- Contents / Nudge：在源采样率与帧数可查时反向移动源偏移，保持 item 的时间线位置和长度。原厂 REAPER 7.82 探针确认整份 10 秒 loop source 在两个边界绕回；非循环源允许正偏移超过 EOF，也允许偏移落到源起点之前。AAADAW 现在允许正向越过 EOF，并由媒体工作线程为 item 剩余时长送入静音，因此正常 EOF 不会被计作实时流 underrun；负源偏移仍因 Core 暴露 `u64` 且 SQLite schema 对该列设有非负 CHECK 而安全拒绝，形成明确的原厂行为差异。显式 bounded source section 的原厂边界行为仍未验证。
- 这些目标的 Set/Snap 组合和其它未验证边界继续明确报告未实现，不修改工程。

单元回归覆盖混合 Audio/MIDI 位置移动、音频目标边界语义、44.1 kHz 源偏移换算、非循环负偏移安全拒绝、正偏移越过 EOF、短源 item 尾部静音且不报 underrun、有界循环段绕回、混合副本选择、Undo/Redo、Set、首项 Snap 锚点与间距、编辑光标路径和不支持目标。实窗中对选中 MIDI 项先执行默认 1 ms Nudge，再设为 250 ms 并点击 `Nudge right`；摘要由 0.001 秒更新为 0.251 秒，编排项也可见地右移，见[操作后截图](fixtures/item-nudge-aaadaw-after-250ms.png)。AAADAW 按统一事务记录 item Nudge；原厂 Contents Nudge 探针的 state-change 计数增量为 0，Undo 历史仍需专项比较。原厂目标实测值见[目标探针 TSV](fixtures/item-nudge-targets-native.tsv)和[Contents 边界记录](fixtures/item-nudge-contents-native.tsv)。REAPER 实窗边界截图见[非循环正偏移越过 EOF](fixtures/item-nudge-native-contents-nonloop-past-eof.png)和[非循环负偏移](fixtures/item-nudge-native-contents-negative-offset.png)；整份 loop source 的两侧绕回数值见 TSV。

## 实窗对照

截图来自实际运行的 REAPER 与 AAADAW 窗口，不是绘图或组件模拟。原厂 Item 菜单截图显示 `Nudge/set items...` 和 `N`；AAADAW 对应实窗截图显示同名菜单项和快捷键。默认对话框尺寸均为 528×121。Target 菜单中原厂一次展开可见八项，AAADAW 在同一大小下滚动显示前四项和后四项。

原厂参考：[默认对话框](fixtures/item-nudge-native-dialog.png)、[Target 下拉](fixtures/item-nudge-native-targets.png)、[Item 菜单](fixtures/menu-item-select-all-native.png)。AAADAW：[选中 MIDI 项时的对话框](fixtures/item-nudge-aaadaw-dialog.png)、[菜单项](fixtures/item-nudge-aaadaw-menu.png)、[Target 前四项](fixtures/item-nudge-aaadaw-targets-top.png)、[Target 后四项](fixtures/item-nudge-aaadaw-targets-bottom.png)。显示环境、操作记录和各文件 SHA-256 见 [fixture 元数据](fixtures/item-nudge.tsv)。

当前明显差异是主题颜色和控件绘制、原厂完整 Target 列表与 AAADAW 的滚动列表、Set 模式布局/字段、错误反馈，以及循环源、目标组合和 MIDI 目标的覆盖。窗口尺寸和基础控件排列相符并不代表视觉或交互已完全复刻。

## 后续验收

下一步为 signed source offset 补齐 Core、SQLite 持久化和媒体流读取，使 AAADAW 能表达并播放原厂允许的负偏移；随后验证显式 bounded section 的原厂 loop 行为。继续探测多种 MIDI target、Set/Snap 组合、所有 Unit、Get cursor、按钮与键盘焦点行为，多选锚点、跨轨顺序、播放/录制状态和 Undo 语义，并在同尺寸实窗比较各模式。完成这些行为和画面对照前，`ITEM-NUDGE-SET-001` 与 Item 菜单/UI 总体验收继续为 `in_progress`。
