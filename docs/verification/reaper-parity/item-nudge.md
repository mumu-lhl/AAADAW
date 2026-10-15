# Item > Nudge/set items...

本记录覆盖 Linux REAPER `7.82/linux-x86_64`、Default 7 主题下 Main action 41226 `Item edit: Nudge/Set...`，以及 AAADAW 当前实现。目标是逐个选项、逐种编辑模式和快捷键行为继续对齐；本文列出的部分行为仍未达到原厂等价，矩阵状态保持 `in_progress`。

## 原厂参考

默认窗口为 528×121。初始状态是 `Nudge`、目标 `position`、`by: 1 milliseconds`、`Snap to unit` 关闭。Target 下拉依次列出 `position`、`left trim`、`left edge`、`end position`、`right trim`、`contents`、`duplicate`、`edit cursor`。Item 菜单在 `Select all` 后显示 `Nudge/set items...` 和 `N`；对应 Main action 是 41226。实际默认菜单截图还能看到其余未实现条目，因此它同时作为 Item 菜单尚未完整的证据。

原厂操作探针用 action 41249 `Item edit: Nudge right by last nudge dialog settings` 验证默认位置 Nudge：选中的 10 秒音频项从 0 移至 1 ms、长度和源偏移不变；选中的 5 秒 MIDI 项也从 0 移至 1 ms、长度不变；没有选中项时素材保持不变。三种情况都保留已有选择。REAPER 的 state-change 计数增量为 0，Undo 栈仍显示 `Insert media items`，没有独立的 Nudge 撤销记录。完整实测值见 [原厂 TSV](fixtures/item-nudge-native.tsv)。

原厂 MIDI Contents Nudge 探针另用一个五秒循环 MIDI item 和两条 MIDI note 验证 250 ms 操作。Target 为 `contents` 时，action 41249 将 `D_STARTOFFS` 从 0 绕至 4.75 秒；action 41250 从 4.75 秒移至 0，再从 0 移至 0.25 秒。item 仍在 2 秒、长度仍为 5 秒，note 的原始 PPQ 范围不变，state-change 计数增量为 0。Snap 开/关在这些 250 ms 案例中的结果相同。详见 [MIDI Contents Nudge 验证记录](item-nudge-midi-contents.md)。

## AAADAW 当前覆盖

AAADAW 的 Action List 入口 `Item edit: Nudge/Set...` 与菜单短标签 `Nudge/set items...` 现在共用一个动作 ID；菜单默认快捷键显示 `N`。Nudge/Set 子窗口为 528×121，Mode、Target、数值和 Unit 按原厂顺序排列，两个 target 菜单截图合起来显示完整八项。

已实现的编辑路径：

- Position / Nudge：同时移动所选 Audio 与 MIDI 项，保持长度、源偏移和 MIDI 内容；多项用一个共享 delta，间距不变，并作为一个可撤销工程事务。
- Position / Set：支持绝对位置和相对位移；多选时以最早的所选项为锚点，对其它项应用同一位移。
- Edit cursor：支持 Nudge 和绝对/相对 Set，不创建工程素材事务。
- Audio / MIDI / Left Trim、Left Edge、End Position、Right Trim：Nudge 按步进量调整对应边界；普通 Set 将每个所选项对应边界放到同一绝对目标，End Position 的 Relative Set 则以最后一个所选项的右边界计算共同位移。Left Trim 对 Audio 按边界距离及素材采样率推进源偏移；loop-source MIDI 按有符号位移更新 PPQ source phase，负向 trim 绕回源循环；non-loop MIDI 在边界穿过源事件时重基准事件和源长度。Left Edge 保留非循环 MIDI 的 source length；End Position 只移动 item；Right Trim 将 non-loop MIDI source length 更新为 source offset 加新的可见长度。所有所选项通过一个事务提交，错误 trim 整批拒绝。Linux REAPER 7.82 的 Audio、loop-source MIDI 与 non-loop MIDI 四种 target 错开多选结果、混合 Audio/MIDI Left Trim、控件显示和撤销已记录，见[Set 边界目标对照](item-nudge-set-edge-targets.md)；mixed-media 错开选择、其它 Unit/Snap 组合和变速 MIDI 仍未全部对照。
- Duplicate：Nudge 按步进量复制所选 Audio 和 MIDI 项；Set 将第一项副本定位到绝对目标，保留多选间距并启用 Snap 取整，副本成为唯一选择。两条路径都通过一个工程事务支持 Undo。Set 的混合媒体用例、原厂音频路径和更多 Unit 仍待验证，见 [Set Duplicate 原厂对照](item-nudge-set-duplicate.md)。
- Contents / Nudge：在源采样率可查时反向移动源偏移，保持 item 的时间线位置和长度。原厂 REAPER 7.82 探针确认整份 10 秒 loop source 在两个边界绕回；非循环源允许正偏移超过 EOF，也允许偏移落到源起点之前。AAADAW 现以有符号源帧保存非循环偏移：负偏移由媒体 feeder 先输出对应静音，再从媒体帧 0 解码；越过 EOF 时 feeder 将 item 尾部补静音。Seek 会跳过已过去的 pre-roll，数据库往返保留符号；loop source 仍拒绝负偏移。REAPER 对关闭 loop 的 bounded section 同样允许负 `D_STARTOFFS`，且该偏移以 section 起点为零点；AAADAW 现保存 section 内 signed 偏移，先输出 pre-roll 静音，再单次播放 bounded section。原厂实测和未验证边界见 [bounded section 验证记录](item-nudge-bounded-section.md)。有限队列回压仍在 feeder 线程执行，实时 callback 未改动。
- MIDI / Contents Set：loop source 按 Tempo Map 计算并绕回 MIDI source phase；non-loop source 在目标早于 item 起点时设置正 source offset，在目标晚于起点时平移 MIDI notes/controllers/pitch bends 并延长源数据。Audio、looped MIDI 和 non-loop MIDI 的 Set 路径都通过事务提交；对应原厂边界见[looped MIDI](item-nudge-midi-set-contents.md)、[non-loop MIDI](item-nudge-midi-nonloop-set-contents.md)、[音频](item-nudge-audio-set-contents.md)和[混合选择](item-nudge-mixed-set-contents.md)。线性渐变 Set 已有一组相位原厂对照；源循环长度与时间基准精度、其他 TempoCurve、其他 Snap 单位和包含 non-loop MIDI 的混合选择仍开放。
- MIDI / Contents Nudge：loop source 的 `source_offset_ticks` 按 item 起点对应 Tempo Map 将毫秒差换算为 PPQ，再按固定 source loop length 绕回；item 时间线边界与源 note PPQ 保持不变。Snap to unit 开/关时都执行同一操作，符合原厂 MIDI probe；AAADAW 不会因该复选框启用而拒绝 MIDI Contents Nudge。Audio 与 looped MIDI 可混合选择并由一个 batch transaction 原子提交，应用回归覆盖一起移动、Undo/Redo 和失败回滚；REAPER 对混合 Contents 选择的原厂结果尚待 probe。线性速度渐变下已改为按源时间换算，但结果仍与 REAPER 相差 6.985 PPQ，原因是 source-loop 长度精度；其他 TempoCurve、旧 trimmed source 长度恢复和完整 Undo 语义仍未完成，见[线性速度渐变验证](item-nudge-midi-linear-tempo.md)。
- 这些目标的 Set/Snap 组合和其它未验证边界继续明确报告未实现，不修改工程。

定向回归覆盖混合 Audio/MIDI 位置移动、音频目标边界语义、44.1 kHz 源偏移换算、负偏移的 feeder 静音与 seek、SQLite 迁移/重开、item 属性编辑与 waveform 起点、正偏移越过 EOF、短源 item 尾部静音且不报 underrun、有界循环段绕回、混合副本选择、Undo/Redo、Set、首项 Snap 锚点与间距、编辑光标路径和不支持目标。实窗中对选中 MIDI 项先执行默认 1 ms Nudge，再设为 250 ms 并点击 `Nudge right`；摘要由 0.001 秒更新为 0.251 秒，编排项也可见地右移，见[操作后截图](fixtures/item-nudge-aaadaw-after-250ms.png)。AAADAW 按统一事务记录 item Nudge；原厂 Contents Nudge 探针的 state-change 计数增量为 0，Undo 历史仍需专项比较。原厂目标实测值见[目标探针 TSV](fixtures/item-nudge-targets-native.tsv)和[Contents 边界记录](fixtures/item-nudge-contents-native.tsv)。REAPER 实窗边界截图见[非循环正偏移越过 EOF](fixtures/item-nudge-native-contents-nonloop-past-eof.png)和[非循环负偏移](fixtures/item-nudge-native-contents-negative-offset.png)；整份 loop source 的两侧绕回数值见 TSV。

## 实窗对照

截图来自实际运行的 REAPER 与 AAADAW 窗口，不是绘图或组件模拟。原厂 Item 菜单截图显示 `Nudge/set items...` 和 `N`；AAADAW 对应实窗截图显示同名菜单项和快捷键。默认对话框尺寸均为 528×121。Target 菜单中原厂一次展开可见八项，AAADAW 在同一大小下滚动显示前四项和后四项。

原厂参考：[默认对话框](fixtures/item-nudge-native-dialog.png)、[Target 下拉](fixtures/item-nudge-native-targets.png)、[Item 菜单](fixtures/menu-item-select-all-native.png)。AAADAW：[选中 MIDI 项时的对话框](fixtures/item-nudge-aaadaw-dialog.png)、[菜单项](fixtures/item-nudge-aaadaw-menu.png)、[Target 前四项](fixtures/item-nudge-aaadaw-targets-top.png)、[Target 后四项](fixtures/item-nudge-aaadaw-targets-bottom.png)。显示环境、操作记录和各文件 SHA-256 见 [fixture 元数据](fixtures/item-nudge.tsv)。

当前明显差异是主题颜色和控件绘制、原厂完整 Target 列表与 AAADAW 的滚动列表、Set 模式布局/字段、错误反馈，以及其他 Unit、Snap、变速 MIDI 和 mixed-media 错开选择的数值已覆盖，跨媒体 UI 和更广的撤销行为仍需检查。窗口尺寸和基础控件排列相符并不代表视觉或交互已完全复刻。

## 后续验收

MIDI Item Properties 的 Loop source 开关已可应用并撤销；AAADAW 现在增加 source loop length 的 PPQ 编辑字段，事务、播放保护、Undo/Redo 与“长度不得切断已有事件”回归已接入。该字段尚未与 REAPER 的原厂控件和缩短/空 trimmed source 恢复语义对照，不能视为完成原厂等价。MIDI Contents Nudge 已完成 stepped Tempo Point 与 mixed Audio/MIDI selection 原厂对照，linear tempo ramp 的 Set 相位现有一组原厂对照；源循环长度与时间基准精度及更广的 Undo 边界仍开放。Set 边界现有 Audio 与 loop-source MIDI 四种原厂数值，以及 staggered non-loop MIDI 四种 target 和 mixed-media Left Trim；mixed-media 错开选择、其它 Unit/Snap 组合和变速 MIDI 仍开放。继续比较其它 Target 的 Set/Snap 组合、所有 Unit、Get cursor、按钮与键盘焦点、跨轨顺序及播放/录制状态。负 offset 属性窗同尺寸实窗截图对照，以及 bounded-section pre-roll 逐采样比较也仍需完成。完成这些行为和画面对照前，`ITEM-NUDGE-SET-001` 与 Item 菜单/UI 总体验收继续为 `in_progress`。
