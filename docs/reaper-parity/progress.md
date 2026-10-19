# 对齐进度

## 会话记录

| 字段 | 状态 |
| --- | --- |
| 用户批准 | 2026-10-09；Linux / REAPER 7.82 / 默认主题与配置 |
| 调研提交 | `8cff888a44b67590f0969a3ece82d4ad9e9e0dba` |
| 唯一开发分支 | `feat/reaper-parity` |
| 唯一 PR | [#282](https://github.com/mumu-lhl/AAADAW/pull/282)，Draft；后续阶段只更新此 PR |
| 当前交付 | 唯一开发分支与 Draft PR #282 持续推进；P4 已提交 Contents Nudge 的原厂边界与 EOF 静音证据，并已接入 schema 49 有符号 source offset、负偏移 pre-roll/seek、持久化、属性编辑与波形显示；looped MIDI Contents Nudge 已贯穿 source-loop 模型，Snap 开/关一致性已按原厂 probe 修正，non-loop MIDI Contents Nudge 已按原厂 source boundary 对齐并覆盖 notes/CC/pitch bend、PPQ clamp、source offset remainder 与单步 Undo/Redo；Audio、混合 Audio/looped-MIDI 与 non-loop MIDI 的绝对 Set Contents 已有原厂对照和单事务 Undo/Redo；已有 P10 纵向切片覆盖 Project Settings Notes、General BPM/拍号/采样率、有符号 Start measure、BPM basis、标尺间隔锚点、Project start time 显示偏移、受支持字段的默认工程设置保存、64-tap sinc 播放、192-tap sinc 离线渲染、Entire project/Time selection/Custom time range/All project regions/All project markers 范围、默认关闭的毫秒尾音控制、Project/常用预设输出采样率转换、Mono/Stereo WAV、PCM8/16/24/32、Float32/64、4-bit IMA ADPCM、2-bit cADPCM 与 8-bit u-Law、可选 sample-peak 归一化（−60 至 0 dBFS）、Selected tracks (stems)、pre-fader stems、Selected tracks via master 与 Master mix and stems；另有 Project Marker 稳定身份/可见编号、schema36 保存/迁移、插入/导航/删除/快捷动作/拖动/属性编辑；Loop Ruler 已补充 Shift 整段移动/Snap、底栏建点、项目级 Loop/Time Selection 保存恢复、全局 link 偏好持久化、Action List link on/off 状态与 40632/40633 范围导航；Repeat 按钮/Action R 已接入 Transport，MIDI 与受限 PCM loop playback 有回归及原厂 start-after-loop 参照；工程 MIDI Note/CC/Pitch Bend 通道已进入 schema 59，MIDI Editor 默认网格已对齐 1/32，并接入选中音符起点/起点+结尾网格量化、Quantize Events 五种 note timing 操作、Strength/Bypass/Fix overlaps、对 modeled note/CC/Pitch Bend 的 All events 与 Swing 网格支持，以及通道选择、Filter、多通道集合与绘制目标；Selected events 已实现；Fix overlaps 关闭时，已测量一个已选音符的移动 note-off 越过下一未选同音邻接音符的原厂读回，并补充 Core/App 回归；未建模事件、复杂混合选择链、Humanize 控件、可撤销编辑与 OK 后全局设置保存已实现，但随机分布仍未验证；cADPCM 编码、Repeat 持久化与完整 loop/原厂验收仍未完成 |
| 最新里程碑 | `Set filter from selection` 已覆盖 Note、同/混合参数 CC、Pitch Bend、Program Change、Poly Aftertouch、Channel Pressure，以及已测 Note/CC/Pitch Bend/Program Change/Poly Aftertouch/Channel Pressure 与 Text Meta 的异构组合；本次增加 CC + Program Change + Text Meta，REAPER 7.82 实测 Show-only 关闭、漏斗开启时通过 9 行、关漏斗恢复 12 行，MIDI 原始数据 201 字节前后相同。新重启的 Cranelift AAADAW 实窗复核相同行为，AAADAW package Nextest 862/862、严格 all-target Clippy、fmt/diff 与两份 manifest 的 21 项哈希检查通过。Position 小数字段与 REAPER 原生组合框显示仍有差异，其他组合仍开放，见下方 2026-10-19 条目 |
| 产品覆盖率 | 尚不可计算：P0 原子清单未封闭 |

## 阶段

| 阶段 | 状态 | 证据 / 下一步 |
| --- | --- | --- |
| 文档落地 | 已完成 | [主实施计划](../plans/reaper-parity-implementation.md)；只表示计划已写入 |
| P0 | 进行中 | [实机动作目录](../verification/reaper-parity/linux-7.82/README.md)已采集；界面/鼠标/偏好和验收预算未封闭 |
| P1 | 进行中 | [快捷键基础能力](../verification/reaper-parity/keyboard-foundation.md)；Action List Filter 与轨道名焦点的首批原厂键盘行为已核验并纳入回归；Section、完整 Action List、数字键盘/物理键和事务仍待推进 |
| P2 | 进行中 | Desktop/Touch profile 已分离；Arrange/Mixer/Media Browser 同时显示、独立分割比例及布局持久化；浮动 Docker 等仍待开发 |
| P3 | 进行中 | 普通轨道输出目标、接收路径 Solo、默认 0 dB 声像与旧工程策略迁移；并行 Post-fader Send/Receive、三种 Tap、嵌套文件夹/Parent Send 与压缩基础已实现；MIDI Send All→All 及 MIDI 沿启用的 Folder Parent 主输出传播已贯通并有原厂事件计数 probe；实时与工程 MIDI Note/CC/Pitch Bend 已保留 0..15 通道并迁移旧工程到通道 0；Send 通道筛选/重映射、MIDI Bus、硬件 I/O、反馈/PDC 与完整路由界面仍待开发 |
| P4 | 进行中 | Item 手动淡化曲线及工程/DSP 基础；Item Nudge/Set 已覆盖多目标、Duplicate 与 Audio/MIDI Contents；schema 50 signed section-relative audio source offset 已跨 Core/Storage/Media/UI 接入，schema 51 MIDI source loop length 与重复事件已接入 Core/Storage/playback/timeline preview/export；looped MIDI Contents Nudge 与 Snap 开/关、non-loop MIDI Contents Nudge 的 PPQ clamp/source offset remainder/notes/CC/pitch bend 与单步 Undo/Redo、mixed Audio/MIDI selection、stepped-tempo 起点及一个 linear-tempo ramp 案例已有 REAPER 7.82 对照和回归；Linear TempoCurve 已改为按项目时间插值，linear-ramp Set/Contents 与 non-loop edge MIDI event positions/source lengths 现匹配原厂整数 PPQ 值，sub-PPQ source-loop/offset remainder 仍开放；AAADAW MIDI Properties 的 PPQ source-loop length 编辑已接入但原厂控件与缩短规则仍未验证、旧 trimmed source 长度恢复、looped MIDI 的绝对 Set→Contents（Snap off 与整秒 Snap-on）、Audio Set、mixed Audio/looped-MIDI Set 和 non-loop MIDI Set 已按原厂值实现；其他 Target 与更广的 Undo 仍待补；MIDI Item Properties 的 Loop source 开关已实现、实窗对照且可撤销，关闭时保留 MIDI source loop length；MIDI Editor 钢琴卷帘/CC/Pitch Bend 循环实例显示、命中与源事件编辑已有回归和原厂/AAADAW实窗截图；Project Marker 插入/导航/删除/编号动作/可见性/标尺选择拖动/属性窗/基础 Manager，以及 Project Region 数据、可见性、时间选择插入、Manager 属性、基础时间尺显示和 Region Render Matrix 的逐区域 Master/轨道分配（schema 53、撤销/重做）已接入；一组确定性原厂矩阵音频输出已与 AAADAW 全 PCM 样本对齐，基础 filename-pattern 字段、wildcard expansion/picker 和单轨/区域范围输出已实现，完整命名语法与编辑交互仍待覆盖；LoopPoints 独立状态/链接动作、Action List on/off 状态、40632/40633 Loop 边界导航、Ruler 手势、项目级范围与全局 link 偏好保存恢复已接入；播放中 play cursor、Repeat、Region 原厂交互、tempo/timebase、GUI 边界与自动 Crossfade 待推进 |
| P5 | 进行中 | [音频 Item 多 Take](../verification/reaper-parity/audio-item-takes.md)：Core/存储、单音频 Item 切换、单源复制/粘贴、活动 Take 删除、40131 Crop to active take 和 41380 Move active takes to top 已接入；REAPER 7.82 原厂验证删除首/末/唯一 Take、裁切两种活动索引与单 Take 无操作、活动 Take 移到顶部和 Undo/Redo；[粘贴](../verification/reaper-parity/audio-paste-as-takes.md)、[删除](../verification/reaper-parity/audio-delete-active-take.md)、[裁切](../verification/reaper-parity/audio-crop-to-active-take.md)和[重排](../verification/reaper-parity/audio-move-active-take-to-top.md)证据。Lane/Comp、录音生成、跨工程剪贴板、40131 之外的多选 Take 操作和其他 Take 顺序操作仍开放 |
| P6 | 进行中 | 工厂 Play/Stop 与 Play/Pause 已分离；Transport 分段规划已接入 `AudioRenderGraph` callback，in-process MIDI instrument/output 可在 loop 回绕处先释放 sustain/跨界音符再 chase 新起点状态，且 callback 分配探针覆盖该路径；Repeat 按钮/Action R 与 MIDI/受限 PCM feeder 已接入，原厂确认从 Loop 后起播线性前进且 AAADAW 音频回归覆盖；Loop 可达时不匹配的 PCM item 与 isolated instrument helper 仍在消费前拒绝，完整原厂音频等价、Repeat 持久化、完整 Transport 状态及录音/设备行为仍待实现/实测 |
| P7 | 进行中 | 已接入 schema 59 MIDI 通道身份、MIDI Editor All/单通道/多通道筛选与绘制目标通道；Quantize Events 支持五种 note timing 操作、Strength/Bypass/Fix overlaps、All/Selected events 与 active Swing grid；REAPER 7.82 原厂已核验 text Meta/F0 SysEx 的 All/Selected Position 量化及同 tick 重复保留；独立 unmatched F7 未能从原厂 raw take 读回；已对关闭 Fix overlaps 时一个选中音符跨越下一未选同音音符的 note-off 读回重配完成 REAPER 探针和 Core/App 回归，并记录事件/截图；MIDI note semitone/octave Action List 动作及原厂默认快捷键已接入，范围边界整批拒绝通过原厂探针验证；Transpose MIDI 对话框已接入 Selected/All notes、半音实时预览、Cancel 回滚与单次 Undo 提交，Snap to scale 和 From/To scale/key 仍待实现；Humanize 对话框 scope/数值的 OK/Cancel 全局记忆已按原厂探针接入；Program Change 的 0–127 值现贯通 Core/SQLite schema 60/SMF 导出/实时调度/CLAP IPC 与输出/Event List，Bank MSB/LSB 属性窗现通过 CC0/CC32 支持并保持单步 Undo，CC0/CC32 在同 tick 会先于 Program Change 输出，事件改动会使已暂停播放图重建；原厂编辑读回与 patch-file 选择仍开放；Poly Aftertouch/Channel Pressure 的模型、存储、SMF、播放、Event List 与属性编辑已有基础覆盖，Meta/SysEx 已支持导入、基础编辑与 CLAP 播放；物理 MIDI SysEx 输出、Humanize 精确 timing/velocity/bias 随机模型、复杂混合选择重叠链与 Piano Roll 工具/颜色/窗口、Pool、Inline/Drum/Notation 仍待实现 |
| P8 | 未开始 | 自动化 |
| P9 | 未开始 | 插件 |
| P10 | 进行中 | 工程帧率/时间码、Notes 与 General 起始速度/拍号/采样率/有符号 Start measure/BPM basis/标尺间隔锚点/Project start time 显示偏移/已实现字段的默认工程设置保存；媒体默认已有 64pt playback/192pt render sinc，Render 支持 Entire project/Time selection/Custom time range/All project regions/All project markers、默认关闭且整数毫秒输入的 Tail、Project/常用预设输出采样率转换、Mono/Stereo WAV、PCM8/16/24/32、Float32/64、4-bit IMA ADPCM、2-bit cADPCM 与 8-bit u-Law、可调 sample-peak、Selected tracks (stems)/pre-fader/via master、Master mix and stems，以及 Region Render Matrix 的 All/Selected Regions 批量任务构造、一组 4 文件原厂 PCM24 对照和 50 个 Wildcard token（其中 18 个 Position/Length 项及 5 个输出格式/通道值与 REAPER 7.82 实窗输出比对；原厂大小写、字符替换/删除、[<N] 截取、空值分隔符、named project-note、numbered region/marker 与空名对象两位编号回退规则已接入）；$region-$track 原厂文件名与完整 PCM 已一致，完整 Wildcards chooser/语法/编辑交互、跨启动保存及其他路由行为仍待验收；cADPCM 编码尚未逐字节一致、Selected regions/markers、其他 Bounds、多声道、其他归一化/限制后处理和完整原厂对照仍缺 |
| P11 | 未开始 | 其他原厂界面 |
| P12 | 未开始 | 全量验收 |

## 实际验证与待补证据

2026-10-09 首批实现 `INPUT-BIND-001` / `UI-030`，提交 `2288d67`：`app/shortcut.rs`、`commands.rs`、键盘事件分发、Settings 追加按钮及配置回归。支持精确修饰组合、字符/数字/标点、功能/导航键、多绑定、语义冲突检查。保留旧配置；工程 schema 为 17。代码提交与证据均在唯一分支，可通过 Git 历史追溯。

`cargo xtest` 全 workspace：715 passed / 0 skipped；Clippy（warnings denied）、默认构建、JACK/PipeWire all-targets check、格式检查通过。GUI 验证追加绑定、冲突保留、保存、重新启动及新绑定撤销轨道；详见 [AAADAW-INPUT-001](../verification/reaper-parity/keyboard-foundation.md)。不把功能回归当作 REAPER 全量行为验收；相关矩阵仍为 `in_progress`。

已确认 REAPER 7.82 安装包 SHA-256、版本、主题和六个 Sections：10,640 条动作目录记录（含重复 Section）、488 个显式绑定。当前虚拟显示无真实音频设备，无法据此验收录音/延迟；鼠标、偏好全树、DPI、多屏、像素差异与其他原子清单仍待 P0 采集。

下一步：继续展开动作/窗口入口及默认输入行为，建立 Section 和动作身份映射，再按等价动作语义迁移默认快捷键；补数字键盘/物理键和完整 Action List。P3 已开始、P4–P12 未开始，100% 对齐尚未完成。

## 后续更新格式

每个切片追加：日期、矩阵 ID、提交、实现模块、迁移版本、运行的验证、结果/证据、剩余差异和下一步。更新 CSV 原子状态，同时更新所属类别摘要；不得因新增文档把条目从待调查改为通过。

新增范围、标准或参照变更记录用户决定和日期。未批准差异保持未完成。P12 完成前不将唯一 PR 标记为产品全量对齐已完成。

## 2026-10-09 继续推进

新增独立动作列表、查找/单独增删快捷键、数字键盘完整输入身份、F10 路由与执行守卫、Project 历史可用性及明确的桌面/触控 profile；[AAADAW-INPUT-002](../verification/reaper-parity/action-list-and-input.md) 记录实际参照及已知差异。727 项 workspace 测试全部通过，默认 Clippy warnings denied 通过。Android CI 的系统 ANR 已定位到软件模拟/无 KVM 权限并修复配置，等待远端复验。独立审查发现的忙碌、保存确认、NumPad5 和错误反馈问题均已补回归。P1 和 P2 继续推进，未关闭全量阶段。

## 2026-10-09 桌面布局基础

桌面默认显示 Arrange 与 Mixer，Master 在 MCP 左侧；Ctrl+M 独立切换 Mixer，Media Browser 与 Mixer 可以同时打开，Transport 位于 Arrange 底部。布局原子保存、空闲防抖、恢复与无效配置保护已实现。新增工厂快捷键让位于用户显式绑定，显示与实际分发一致。732 项 workspace 测试、Clippy warnings denied 通过。Android 模拟器 CI 两次通过（37958395698、37958403612）。[布局验证](../verification/reaper-parity/desktop-layout.md)记录 GUI 证据与未完成范围。

## 2026-10-09 通用输出与声像策略

消除专用 Bus 输出目标限制，普通轨道可作为单输出接收目标；保持缺失/自路由/环路/删除依赖保护与撤销、存储往返。Solo 上游只开放接收路径，不放行接收轨道自身内容。REAPER 实测默认 Pan Law 0 dB，半左/半右为线性 balance；新工程使用 ZeroDbBalance。schema 18 新增工程 pan_mode，旧工程迁移为 LegacyMonoStereo，原 Bus 音频路径与历史数值 fixtures 保留。两种策略均验证接入静音路由前后自身 mono 增益一致，不做 FX 前的局部增益补偿。[路由验证](../verification/reaper-parity/ordinary-track-routing.md)记录参照输出及未完成项；该切片不是并行 Send/Receive，也未关闭 P3。

## 2026-10-09 并行音频发送

稳定 SendId、重复目标、独立主输出开关、发送参数和 Receive 反向视图已实现；schema 19 保留旧工程路由。预分配扇出与路径 Solo 有音频回归，默认 Mixer 的 IO 入口移到轨道头部。756 项 workspace 测试及默认 Clippy 通过；[验证与剩余差异](../verification/reaper-parity/audio-sends.md)明确记录当前 Post-fader 限制、MIDI 动态恢复与 GUI 原生保存验证缺口。P3 仍在进行中，后续继续 Tap Point 与文件夹路径。

## 2026-10-09 发送位置

三种 Audio Send Tap 已实现，schema 20 将旧发送保留为 Post-fader。REAPER 常量探针与合成 FX 边界回归通过；完整 759 项测试与 Clippy 通过，GUI 独立切换位置有证据。详见 [发送位置验证](../verification/reaper-parity/audio-send-taps.md)。下一步文件夹 Parent Send，P3 仍在进行。

## 2026-10-09 文件夹与 Parent 路径基础

稳定父子关系、完整子树移动、有效 Parent Send、TCP/MCP 同步与 schema 21 已实现，旧 Bus 保留。767 项测试通过；[文件夹验证](../verification/reaper-parity/folder-parent-routing.md)记录实际嵌套音频、GUI 和待做压缩显示。Windows 预听 CI 发现固定轮数异步测试的调度缺口，[修正证据](../verification/reaper-parity/isolated-preview-ci.md)记录 focused/stress 通过及远端待验。P3 继续进行。

## 2026-10-09 文件夹三态压缩

Normal/Small/Tiny 循环、25/4 px 后代显示、Item 边界、隐藏及恢复自动化编辑区域和 schema 22 视图持久化已实现。771 项测试、Clippy 与构建通过；GUI 重启恢复与展开保留媒体通过。[证据](../verification/reaper-parity/folder-parent-routing.md)保留普通高度、拖动手势和完整主题差异。前一提交 Windows 预听复验及全平台 Rust CI 已成功。P3 继续进行，P4–P12 未开始。

## 2026-10-09 MIDI 实时恢复

构建时被静音/Solo 排除的内部乐器现在保留完整计划，播放中解除排除可恢复；调用方输出使用当前状态过滤并保留释放事件。修改前失败的回归已通过，773 项测试、Clippy、20 次 stress 通过。[证据与限制](../verification/reaper-parity/midi-live-audibility.md)记录 MIDI 路由/外部追赶/静音 FX 偏好仍未完成。P3 持续推进。

## 2026-10-09 Action List 录入确认

Add 独立键盘录入窗口与手动 OK/Cancel 已实现，取消不写入，保存失败保留草稿，父列表捕获期间不改变动作。774 项全量测试及最后输入守卫的 6 项 focused 回归、Clippy、构建和 GUI 确认/取消通过。[记录](../verification/reaper-parity/action-list-and-input.md)仍保留 Section、重复分配优先级、Scope/MIDI/OSC 等差异。文件夹压缩提交 eeb1b0d 的 Rust CI、Portable、Installers 均已成功。P1/P3 继续进行。

## 2026-10-09 默认传输入口

按动作目录分离 Space Play/Stop 与 Enter/Ctrl+Space Play/Pause，保留旧显式绑定，桌面独立 Pause 按钮。录音准备期间停止命令守卫与取消路径补齐。797 项 audio-device 全量测试、默认 Clippy、音频功能构建与 GUI 入口显示通过。[证据](../verification/reaper-parity/transport-bindings.md)记录无真实设备和完整状态/偏好验收缺口。P6 已开始但未关闭，P0–P3 继续进行。

## 2026-10-09 轨道反相

实机测量证明只影响主输出/Post-fader，Pre-FX/Pre-fader 保持原值。领域、schema 23、实时参数、Undo、IO/Action List 已实现；默认 779 / audio-device 802 项测试、默认 Clippy、构建及 GUI 恢复通过。[证据](../verification/reaper-parity/track-phase.md)保留 TCP/MCP 默认按钮和更多通道/偏好差异。P3 仍在进行。

## 2026-10-09 桌面 MCP 纵向控件

实测 Master/普通条宽 132/88 px，桌面采用纵向推子与双通道表头，加入已实现的反相入口；触控 profile 保留。779 项测试、Clippy、默认构建及音频功能检查通过，GUI 拖动提交、同步、撤销和窄窗有[证据](../verification/reaper-parity/mcp-vertical-layout.md)。Master 真实推子、完整主题和 Meter 语义继续开发，P2/P3 均未关闭。前一反相提交 d93d9ba 的 Rust CI、Portable、Installers 均已成功。

## 2026-10-10 Master 音量与声像

独立 MasterMix、领域撤销、schema 24 迁移、实时原子立体声系数和跨块 ramp、冻结源隔离、真实桌面推子/声像控件已实现。809 项 audio-device 全量测试、最后相关 68 项 focused、默认 Clippy 和音频构建通过；GUI 拖动提交、撤销、Redo、重启 Recover 有[证据](../verification/reaper-parity/master-mix.md)。Master 静音/反相的离线导出观察不等同实时监听规则，完整控件与 FX/Meter/Automation 仍缺。50348df 的 Rust CI、Portable、Installers 均成功；P2/P3 继续推进，后续对齐已实测的 +12 dB / Default 推子范围和位置映射。

## 2026-10-10 Default 推子曲线

公共 API 采样的双向曲线、+12 dB 推子上限、有限零增益端点、TCP/MCP/Master 同步与 -inf 显示已实现；精确录入按实机保留 +20 dB。813 项 audio-device 全量、末次 18 项 focused、Clippy 和构建通过，GUI 端点/重置/撤销及范围外数值有[证据](../verification/reaper-parity/default-fader-curve.md)。网格外插值、自定义形状、原厂灵敏度及完整主题仍未关闭。前一 Master 提交 cd65161 的 Rust CI、Portable、Installers 全部成功；继续推进其余编辑基础。

## 2026-10-10 Item 手动淡化基础推进

`ITEM-FADE-001` 开始实现，参照与边界见 [逐帧证据](../verification/reaper-parity/item-fades.md)。七种曲线、超长/重叠长度规则已用原厂真实输出核对；schema 25 保留旧工程无淡化语义。模型、撤销、复制、存储与真实 PCM 处理已接入，UI 手柄及实时发布仍待实现，不能视为 P4 或全量对齐已完成。

上一推子提交 `f47bfe4` 的 Rust CI、Portable desktop archives、Native installers 三条工作流均已成功。

本地音频功能全量 819 项通过、零跳过；默认 Clippy warnings denied 通过，原厂 12 组输出逐帧校验通过。GUI 淡化和实时发布尚未实现，完整参照与未完成边界保留在证据文档。

## 2026-10-10 Item 淡化实时控制

固定音频图加入按 ItemId 发布的完整手动淡化参数对，callback 一次有界读取，写入中断时沿用上次完整值，无等待、自旋或分配；普通提交、批事务和历史同步已接入。独立淡化事件支持播放中的单项 Undo/Redo。21 项相关回归及最终 audio-device 全量 822 项通过、零跳过，默认 Clippy warnings denied 通过。GUI 淡化手柄、曲线菜单及预览仍待实现；P4 保持进行中。

## 2026-10-10 REAPER 7.82 连续淡化曲率

补测 7.81 后的曲率/S 控件及兼容模式差异，不能仅以七个旧编号代表当前版本全部曲线。新增显式 Legacy/Native 模式、完整实时发布和 schema 26：旧曲线保留，连续曲率按两端参数对保存。25 个原厂输出、2,400,000 个声道采样点逐帧比较通过；最终 audio-device 全量 825 项通过，默认 Clippy warnings denied 通过。证据和边界见 [Item 淡化](../verification/reaper-parity/item-fades.md)。当前 GUI 手柄、连续曲率编辑、自动录音/分割淡化与 Crossfade 仍待实现，P4 未完成。

已确认原厂默认 Fade 拖动忽略 Snap，Shift 操作 Crossfade；复现工具必须发送实际移动事件，单纯指针定位未改变参数。默认 Imported 淡化未勾选，Recorded/Split 勾选 10 ms，但自动处理尚未接入。前一实时提交 `ac0c071` 的 Rust CI、Portable desktop archives、Native installers 全部成功。

## 2026-10-10 时间线淡化手柄

单个音频 Item 的手动淡入/淡出手柄、同模型曲线、实时预览、释放一次提交和 Escape/右键取消已接入。窗口坐标锚点处理选中时布局变化，连续时钟保留小数采样与变速映射；曲线移入时间线 GPU 绘制后通过残留回归。最终 audio-device 全量 828 项、默认 Clippy、针对性回归及窗口操作/保存恢复通过，[证据](../verification/reaper-parity/item-fades.md)保留分组、修饰键、完整属性/曲线编辑和自动 Crossfade 缺口。052a942 的 Rust CI（含 Android 模拟器）、Portable、Installers 均成功。P4 持续进行。

## 2026-10-10 淡化曲线菜单

右键淡化手柄选择七种曲线已接入，实测确认原厂 Smooth 选择切换至新版 S 参数，其余预设保留兼容模式。选择仅改变指定端曲线，长度、另一端和 Undo/Redo 保留；Escape 与菜单外时间线点击关闭。830 项 audio-device 测试、默认 Clippy 通过，实窗及参照证据见 [淡化验证](../verification/reaper-parity/item-fades.md)。schema 仍为 26。低通淡化、连续曲率编辑、菜单键盘导航及 Crossfade 仍待完成，P4 未关闭。d2bfbe2 三项远端流程已成功。

## 2026-10-10 属性窗口淡化字段

单 Audio Item 的独立属性窗口及打开/切换动作接入；F2 在两个窗口的焦点下都可切换，自定义绑定优先。淡化长度及曲率/S 草稿通过 Apply/OK 一次提交，取消无历史；实测补齐属性长度归一化与双端重叠优先规则。833 项 audio-device 测试、默认 Clippy 和最新构建实窗验证通过，schema 仍为 26；[证据及剩余差异](../verification/reaper-parity/item-fades.md)。原厂完整属性窗口和 Item/Take/Source 功能尚待补齐，P4 仍在进行；7ab788a 三项远端流程已成功。

## 2026-10-10 属性位置与源起点

Time 输入下的 Position/Length/Start in source 接入既有整数采样模型，与淡化修改先验证再一次事务提交，保持缩短 Item 时的请求淡化长度；播放中保护结构修改。834 项 audio-device 测试、默认 Clippy、实窗保存和一次 Undo/Redo 通过；[证据](../verification/reaper-parity/item-fades.md)。schema 仍为 26。显示单位、负/分数采样位置、Loop source、完整 Take/属性布局和批量编辑仍待推进，P4 未关闭。

## 2026-10-10 属性显示单位与时间格式

Time / Samples 及配置持久化接入；按实测在切换时刷新全部字段、丢弃未应用草稿并禁用 Apply，保留已应用数值精度。原厂 27 组时间格式事实纳入回归，修正毫秒截取及小时格式。837 项 audio-device 测试、默认 Clippy、最新构建实窗及 Samples 重启恢复通过；[证据](../verification/reaper-parity/item-fades.md)。schema 仍为 26；Beats/H:M:S:F、帧率和完整属性仍待推进。832858b 三项远端流程已成功。

## 2026-10-10 Item Properties Beats

公开 Core 小节/拍/分数拍内 tick 反算接口与边界验证已接入；UI 使用公开 Project 速度/拍号图转换 Beats。位置/长度保留不同起点规则，源起点仍为 Time，未编辑字段保留完整样本精度；修改后位置作为长度解析锚点。新增 27 组显示、标准输入及 15 组跨速度/拍号数值回归，记录原厂跨拍号非互逆行为。实际 GUI 验证 Apply、一次 Undo/Redo、配置重启恢复；详见 [证据](../verification/reaper-parity/item-fades.md)。上一提交 `6ca3cf2` 三套 CI 全部 success：Rust 38075048442、Portable 38075048481、Installers 38075048443。最终 842 项 audio-device workspace 测试全部通过；默认 Clippy warnings denied 和格式/diff 检查通过；H:M:S:F、完整 Item 属性及全量阶段仍待推进。

## 2026-10-10 工程帧率与 Item Properties H:M:S:F

实测十种原厂帧率预设及 121 行格式化/反解析数值，分数帧率精确比率、DF/ND 模式和小时格式独立。Core 帧率设置、DawAction、Undo/Redo、Snapshot 与 schema 27 独立单行表接入；旧 26 工程保留内容并默认 30 fps，只读旧文件保护、重复迁移、非法编码及失败保存整笔回滚已验证。H:M:S:F 使用工程帧率转换，源偏移/淡化仍 Time，未编辑字段保留精度。实际 GUI Apply、一次 Undo/Redo、保存与重启已验证；[证据](../verification/reaper-parity/project-timecode.md)。最终 audio-device workspace 848 passed / 0 skipped、默认 Clippy warnings denied、fmt/diff 检查通过。上一提交 `3111529` 三套 CI 均成功：Rust 38076194172、Portable 38076194162、Installers 38076194190。完整 Project Settings 帧率入口、视频/时间偏移、宽松输入、负/分数采样 Item 时钟和全部阶段仍待实现。

## 2026-10-10 Item Properties 帧率联动修正

新增采样级原厂探针，确认同帧率曲率 Apply 保留 16001/12001/6001 样本；30→25 fps 不刷新窗口草稿，下一次有效 Apply 用新帧率重新解释旧位置/长度文本，干净 OK 则不改变媒体。草稿记录显示时帧率并只在该实测条件下重解析两项，保留原源偏移精度、事务与播放守卫。最终 849 项 audio-device workspace 测试通过；默认 Clippy warnings denied、fmt/diff 检查通过。[数值与说明](../verification/reaper-parity/project-timecode.md)。完整 Project Settings GUI 继续实施，未完成全量阶段。

### Project Settings 帧率入口

File/动作列表/默认 Alt+Enter 接入同一个工程设置窗口，十项帧率使用独立草稿、OK 单次领域动作、Cancel/Escape 放弃，保留自定义快捷键优先、忙碌和跨工程草稿保护。原厂初始 606×535 窗口容纳完整菜单；真实 GUI 验证菜单 Escape、25 fps 提交、一次 Undo/Redo。853 项测试、默认 Clippy warnings denied、fmt/diff 通过；上一提交 43bba31 的 Rust/Portable/Installers CI 均成功。全部页、控件、键盘导航和总体对齐尚未完成，P10 保持进行中。

### Project Settings 帧率菜单键盘

实测 Linux 原厂菜单方向键循环、Enter 确认菜单但保持窗口、Escape 放弃菜单选择。自有菜单状态补齐 Up/Down、首尾循环和 Enter/Escape，鼠标继续复用框架菜单绘制；实时窗口证明与原厂 30→48、23.976→75 序列一致，Escape 保留原选择，鼠标选项仍可提交草稿。855 项 workspace 测试、默认 Clippy warnings denied、fmt/diff 通过。任意帧率输入、闭合焦点和其他导航键、完整各页继续实施，阶段未完成。

## 2026-10-11 Project Settings Notes

实测 Linux 7.82 / Default 7 新配置 Notes 页：独立草稿、Cancel 放弃、OK 接受、Ctrl+Z 不回退 Notes、默认空标题/作者/正文及显示选项、加载时打开 Project Settings 的 Notes 页。Core ProjectNotes/公开接口/DawAction/Snapshot、schema 28 独立表和原子保存迁移接入；Notes 从媒体历史中剔除，保留既有 Redo，混合事务失败回滚，后续 Undo 不丢弃文档。窗口加入 Notes 页、标题/作者/多行正文/折行/加载显示，保存工程和会话恢复自动显示。

861 项 audio-device workspace 测试、默认 Clippy warnings denied、fmt/diff 通过；真实 GUI 验证 Enter、取消、跨页草稿、Notes+25 fps 提交、Undo/Redo、重启恢复自动显示。[证据](../verification/reaper-parity/project-notes.md)。上一提交 305c71e 的 Rust/Portable/Installers CI 均成功。使用已校验安装的 RTK 0.51.0 压缩命令输出；不添加项目运行依赖。完整主题/标签几何、编辑器 Undo/焦点快捷键、各页及全部阶段仍未完成，P10 保持进行中。

### 2026-10-11 Project Settings General 起始速度与拍号

将 General 加入同一设置窗口，读取工程初始 BPM/拍号作为独立草稿，OK 用单一事务提交 `SetTempo@0` 与 `SetTimeSignature@0`，Cancel 丢弃；无效 BPM/拍号不会改工程或关窗。有效组合一次 Undo 可还原。原厂 Linux REAPER 7.82 / Default 7 空工程脚本探针确认 120 BPM、4/4、44100 Hz 且工程采样率未强制；同时确认 item timebase 为 Beats(position, length, rate)，MIDI timebase flags 为 0。General 的采样率、起点、BPM 基准、完整 timebase/resampling/pitch 与 Advanced 控件仍未实现。

真实 AAADAW GUI 实测默认 120/4/4、未确认前草稿不改工程、确认后窗口重新打开显示 96/3/8、Cancel 后一次 Ctrl+Z 恢复 120/4/4；0 BPM 的 Apply 保持窗口打开。自有界面证据：[默认值](../verification/reaper-parity/project-settings-ui/general-default.png)、[待提交草稿](../verification/reaper-parity/project-settings-ui/general-draft.png)、[确认后](../verification/reaper-parity/project-settings-ui/general-after-apply.png)、[撤销后](../verification/reaper-parity/project-settings-ui/general-real-undo.png)、[无效输入仍打开](../verification/reaper-parity/project-settings-ui/general-invalid-result.png)。没有提交 REAPER 原厂截图、工程或媒体。

864 项 audio-device workspace 测试、默认 Clippy warnings denied、fmt/diff 检查通过；随后一项额外的设置页切换回归也通过。对应矩阵 `PROJECT-TEMPO-METER-SETTINGS-001` 仍为 `in_progress`：General 只完成起始速度/拍号子集，P10 与全量对齐继续进行。

### 2026-10-11 Project Settings General 有符号 Start measure

原厂窗口确认 Start measure 支持 0 和负数，-2 保存后重开仍保留，基准尺标签也显示 -2。提交 `d9ad079` 将 AAADAW General 的有符号 32 位 Start measure 草稿与起始 BPM/拍号作为一个事务提交；Core `MusicalPosition` 与反向 tick 换算、Undo/Redo、工程快照和 schema 29 存储迁移均接入。Beats 属性的位置显示/输入支持有符号小节；Item 长度仍拒绝负条数。时间线标尺以工程起始小节为标签间距原点。Schema 28 项目迁移后默认为 1，并验证列已存在时不会重复添加。

真实 AAADAW X11 验证 120/4/4/1 默认值、96/3/8/-2 草稿/提交/重开/一次撤销、0 保存/重开/撤销，以及越界值保持窗口且不改工程；REAPER 原厂 General 窗口验证 0/-2 和基准尺标签。证据与仍未实现的 General 控件列于[阶段记录](project-settings-general.md)。新增 Core、Storage、Beats、Ruler 回归后，audio-device workspace 为 868 passed / 0 skipped；默认 Clippy warnings denied、fmt 和 diff 检查通过。schema 29；矩阵 `PROJECT-TEMPO-METER-SETTINGS-001` 继续 `in_progress`，采样率、起点/timebase/resampling/pitch、全部设置页和全量验收仍待推进。

### 2026-10-11 Project Settings General BPM basis

REAPER 7.82 原厂窗口枚举顺序为 1/8、1/4、1/2、1/8 dotted、1/4 dotted、1/2 dotted，默认 1/4；BPM、拍号与基准选择在同一行。原厂实测将 120 BPM 基准切为 1/8 后，数字仍显示 120，但 4/4 小节时长从 2 秒变为 4 秒，确认基准影响项目音符时钟。AAADAW 将六项接入 General 草稿与一次工程事务；Core TempoMap 的 sample/tick/ramp 换算、tempo 合法性、Undo/Redo、快照和 schema 30 工程存储均包含该设置。旧 schema 29 工程迁移后默认四分音符基准。

最终 audio-device workspace 全量 870 项通过、零跳过；默认 Clippy warnings denied、fmt、diff 和 reference JSON 检查通过。自动化聚焦回归覆盖六项换算、往返、tempo ramp、Undo/Redo 和 schema 29 迁移；真实 AAADAW X11 覆盖选项菜单、1/8 应用/重开/整体撤销、0 小节和拒绝越界输入。General 的采样率、起点、完整 timebase/resampling/pitch 和其余设置页仍待实现，P10 继续进行。

### 2026-10-11 Project Settings General Base ruler label spacing

原厂 7.82 将复选框标题动态显示为 “Base ruler label spacing on measure N”。隔离工程将 Start measure 设为 -2，并在两种状态下缩放核验：启用时等距标签以工程起始小节 -2 为相位，关闭时以小节 1 为相位。AAADAW 加入默认开启的工程设置、事务动作、Undo/Redo、快照恢复和 schema 31；General 草稿在 Start measure 旁展示该开关，标尺间隔计算按所选锚点执行。旧 schema 30 默认启用，保存可 round-trip 关闭状态。

最终 audio-device workspace 全量 874 项通过、零跳过；默认 Clippy warnings denied、fmt、diff 和 reference JSON 检查通过。回归覆盖 Project 默认值、单项与批事务 Undo/Redo、schema 30 迁移、密集与稀疏标尺标签间距；真实 GUI 验证默认开启、Start measure 联动标题、关闭状态的 Apply/重开，以及低缩放时 measure 1 相位。General 采样率需要联动工程采样时钟、媒体位置、录音、插件和设备流，仍在独立设计/实施中；项目起点、其他时基、resampling/pitch 和其余页面继续推进。

### 2026-10-11 Project Settings General Project start time

REAPER 7.82 General 中将 Project start time 设为 `0:10.000` 后，Transport 与尺标时钟平移 10 秒而小节不变；`-0:05.000` 也可保存。AAADAW 将有符号显示偏移接入 ProjectSettings、批事务动作、Undo/Redo、快照和 schema 32；旧 schema 31 默认 0。Transport、尺标、Item Properties Time/H:M:S:F 绝对位置均应用偏移，样本、音乐位置、Item 时长和源偏移保持原值。无效时钟输入阻止整个 General 草稿提交。

真实 GUI 验证默认/草稿/应用重开/整体撤销、`0:60.000` 拒绝、Start measure=0 保存；原厂 scratch 工程验证正负偏移和测量位置。audio-device workspace 全量 884/884 通过；最终 migration/storage 边界的 focused tests、默认 Clippy warnings denied、fmt/diff 和 reference JSON 检查通过。证据及 General 未完成控件见[阶段记录](project-settings-general.md)。矩阵 `PROJECT-TEMPO-METER-SETTINGS-001` 保持 `in_progress`；采样率、其他时基、resampling/pitch、完整几何、其余页面和全量验收继续推进。

### 2026-10-11 Project Settings 起始时间菜单与 Cranelift 开发构建

General 的 `Change start time/measure...` 菜单接入前四个原厂命令：当前编辑光标设为零时钟、重置工程起始时间、最近小节设为小节 1、重置工程起始小节。Up/Down 循环，Enter 执行，Escape 关闭；更改只留在 General 草稿，OK 单次提交、Undo 单次撤销。fractional-frame 起点选项随后单独接入，见下方阶段记录；菜单截图与功能边界见[General 记录](project-settings-general.md)。

Stable audio-device workspace 全量测试、Clippy `-D warnings`、fmt/diff 和 reference JSON/CSV 检查通过。nightly Cranelift 可编译本项目，但不默认启用：本轮 isolated target 的 cold `build -p aaadaw --features audio-device` 耗时 3m52；stable `cargo xtest` 编译约 2m37，profile/target 不同，不能据此断言速度更快。适合后续作为个人本地 debug 选项做同配置增量构建对比；release、CI 和仓库默认仍用 stable/LLVM。

General 仍缺采样率、timebase、resampling、pitch 等原厂控件；矩阵 `PROJECT-TEMPO-METER-SETTINGS-001` 和 P10 继续保持 `in_progress`。

### 2026-10-11 Project sample rate 工程时钟

General 新增常用采样率选择。`SetProjectSampleRate` 通过 Core Action 将工程音频片段位置/长度、淡变、轨道音量自动化、CLAP 参数自动化和 TempoMap sample anchor 按比例换算，保留秒数与媒体源偏移；撤销事件记录精确的换算前状态，支持单次 Undo/Redo 和快照恢复。降低采样率会合并不同自动化点或超出时间线可表达范围时，整项修改失败且不改工程。设置事务会重建播放图；Transport/playback 起点在应用、Undo 和 Redo 时同步换算。

Core 与 UI 专项回归已通过，audio-device workspace 全量测试也已通过。Xvfb 下的隔离 GUI 验证覆盖原速率选择列表、设置应用后重开读回 44.1 kHz，以及从 Edit 菜单 Undo 后恢复 48 kHz；截图见 [默认与采样率](../verification/reaper-parity/project-settings-ui/general-project-sample-rate.png)、[选项列表](../verification/reaper-parity/project-settings-ui/general-project-sample-rate-options.png)、[应用后](../verification/reaper-parity/project-settings-ui/general-project-sample-rate-after-apply.png)和[撤销后](../verification/reaper-parity/project-settings-ui/general-project-sample-rate-after-undo.png)。默认 Clippy `-D warnings`、fmt/diff 检查通过；带 `audio-device` 的严格 Clippy 另外暴露已有的 `unused_mut`、Android-only recovery `dead_code` 和两个测试 `field_reassign_with_default` 警告，均在本切片改动之外。原厂 sample-rate checkbox 默认关闭且“use hardware default”语义、whole-sample 强制、默认 44.1 kHz 与当前 AAADAW 默认 48 kHz 的差异、原厂完整速率列表和几何仍未对齐；本轮选择器只提供可实际作用于工程的显式速率修改，不声称完成采样率控件全量对齐。

### 2026-10-11 Project start fractional-frame flag

REAPER 7.82 原厂菜单确认第五项 `Allow project start to be at a fractional frame offset` 默认关闭。隔离 RPP 对照显示该开关对应 `TIMEMODE` 最后一字段 `0`/`4`；编辑光标为 0.025 秒且开关关闭时，原厂“Set 0:00 to current edit cursor”仍将起始时间设为 -0.025 秒，因此这不是禁止小数秒的开关。AAADAW 将它接入菜单勾选状态、General 草稿、单次提交/Undo、Core 状态和 schema 33；旧 schema 32 默认关闭。其视频/时码对齐效果尚未确认，当前只实现并保存了原厂开关状态，不宣称完整语义对齐。

针对这项切片的 Core、Storage 和 UI 回归覆盖 Undo/事务、schema 32 默认值及 round-trip、非法存储值拒绝和菜单勾选状态。真实 AAADAW X11 窗口验证了默认未勾选、点击后勾选以及菜单重开保留草稿勾选态；对应两张截图已纳入证据。audio-device workspace 全量测试、Clippy `-D warnings`、fmt/diff、reference JSON 和 parity CSV 校验通过。矩阵 `PROJECT-TEMPO-METER-SETTINGS-001` 与 P10 继续 `in_progress`，General 采样率、timebase、resampling、pitch 和其余页面仍待实施。

### 2026-10-11 Project Settings 原厂第一页签标签

原厂窗口核对确认第一页签的完整标签为 `Project Settings`，原来使用的 `General` 是子页简称。AAADAW 将页签文字改为原厂名称并在 X11 实际窗口复核；当前页仍只提供已实现的 General 子集，原厂 Media、Advanced 页签及大量 General 控件继续作为未完成项登记。默认 General 与起始时间菜单截图已更新，不将标签对齐误记为页面完成。

### 2026-10-12 General whole-sample tempo anchor

在隔离 REAPER 7.82 Linux Default 7 工程中对比了 `SAMPLERATE` 的三个字段：factory `44100 0 0`，仅 project sample rate 开启为 `44100 1 0`，whole-sample 也开启为 `44100 1 1`。General 新增同名勾选框；Core `SetForceTempoChangesToWholeSamples` 会在开关变更后重算 TempoMap anchor，启用时将 tempo point 的连续样本位置取整到工程 sample。44.1 kHz / 120 BPM / PPQ 960 下 tick 1 anchor 从 22.96875 变为 23.0。schema 34 新增布尔列，旧 schema 默认关闭；Core、Storage、UI 分别覆盖转换、迁移 round-trip 与事务 Undo/Redo。

三个专项测试通过。Xvfb 真实窗口验证 checkbox 草稿、Apply 与 Undo 后状态恢复，截图见 [勾选](../verification/reaper-parity/project-settings-ui/general-whole-sample-checked.png)和[撤销后](../verification/reaper-parity/project-settings-ui/general-whole-sample-after-undo.png)；原厂 General 默认与勾选参考截图也已存档。原厂鼠标 marker-edit snapping、meter-marker 对该 flag 的具体效果仍需进一步测量；project sample-rate factory enable/hardware-default 语义、timebase、resampling/pitch 和其余页面继续未完成。

### 2026-10-12 原厂工程采样率预设

在 REAPER 7.82 General 原厂采样率下拉菜单中实测到 11 个选项：8,000、11,025、16,000、22,050、32,000、44,100、48,000、88,200、96,000、176,400 和 192,000 Hz。AAADAW 的 General 选择器已补齐原厂顺序；当前工程速率若不在预设中，仍额外显示该值以免设置窗口丢失当前状态。参考列表截图已提交；focused UI/Core 测试覆盖原厂完整顺序。project-rate enabled 与 hardware-default 行为另属后续音频时钟工作。

### 2026-10-12 General 保存默认工程设置

Project Settings 底部新增 `Save as default project settings`。按钮与 OK 共用 General 草稿校验及工程提交路径，然后将当前已实现的设置字段原子保存到用户配置；成功后窗口保持打开。应用启动和 File > New project 加载这些值。新工程只恢复 BPM、拍号、采样率、BPM basis、小节号、尺标间隔、whole-sample、项目起始时间、fractional-frame 和帧率，不复制内容或 Undo 历史。无效草稿不会写入；未知版本/非法字段拒绝加载且保留原配置文件。工程构造新增带初始拍号的无历史入口，保证默认项目可直接开始编辑。

`cargo test -p aaadaw-core --no-default-features`、`cargo test -p aaadaw --no-default-features`（423 个应用单元测试及集成测试）、`cargo clippy -p aaadaw --all-targets --no-default-features -- -D warnings`、`cargo build -p aaadaw --no-default-features`、fmt 与 diff 检查通过。项目默认配置模块测试覆盖文件 round-trip、非法配置保留和新工程内容隔离；UI 测试覆盖有效草稿保存与无效草稿拒绝。真实 Xvfb GUI 将 BPM/拍号改为 96/7/8，保存后应用重启与 File > New project 都恢复该值。本功能仅闭合 AAADAW 已实现字段，不宣称覆盖 REAPER 未实现的 timebase、resampling/pitch、Media/Advanced 或完整模板格式；矩阵与 P10 保持 `in_progress`。

### 2026-10-12 播放重采样基础

REAPER General 播放重采样菜单已保存截图并确认工厂默认 `Sinc Interpolation: 64pt (medium quality)`；render 默认另为 192pt。AAADAW 媒体流工作线程现在使用归一化 64-tap Blackman-windowed sinc，按采样率比例做抗混叠低通，同速流走逐样本直通。该处理保持在解码 worker 内，继续使用有界 PCM 队列；音频回调不承担系数生成或滤波运算。

新增测试覆盖常量电平、输出长度、分块相位连续、立体声通道及 48→24 kHz 抗混叠；文件 Item 集成测试保留裁切、重采样与时间线定位检查。`xvfb-run -a cargo xtest --no-fail-fast` 为 877 passed / 0 skipped，`cargo clippy --workspace --all-targets -- -D warnings` 与格式检查通过。此实现不是经逐系数比对的 REAPER 内核；设置选择与工程持久化、其余十种模式、独立 192pt 离线渲染模式均未实现，见[验证记录](../verification/reaper-parity/project-playback-resampling.md)。General 原子矩阵和 P10 保持 `in_progress`。

## 2026-10-12 播放与离线重采样质量分路

`PROJECT-TEMPO-METER-SETTINGS-001` 切片提交 `70601c67692bd2e15d8fd3a0c4c593d4a5a0e0dd`：REAPER 7.82 默认播放 64pt、Render 192pt 已分别映射到媒体工作线程质量参数；现有播放 API 保持 64pt，WAV 工程导出与 Freeze 显式使用 192pt。不同采样率的归一化 Blackman sinc 系数按速率与质量缓存，并在已知源采样率时于 worker 报 ready 前准备；实时 callback 仍只消费有界 PCM 队列。

覆盖恒定电平、输出长度、分块边界、声道隔离、下采样通带/混叠、192pt 过渡带抑制及项目队列两种质量的测试。`cargo fmt --all -- --check`、media/app Clippy warnings denied、CSV/JSON 结构校验通过；`xvfb-run -a cargo xtest --no-fail-fast` 最终 879/879 passed、0 skipped。证据见[重采样验证记录](../verification/reaper-parity/project-playback-resampling.md)。

General 重采样选择与工程持久化、其他质量/算法、REAPER 系数级等价及 pitch/stretch 行为未完成；P10 与 `PROJECT-TEMPO-METER-SETTINGS-001` 保持 `in_progress`。下一项优先扩展 `RENDER-BOUNDS-001` 的真实 Render 设置和可验证输出边界，不将默认滤波器长度视为完整渲染对齐。

## 2026-10-12 Project Render 范围与尾音基础

`RENDER-BOUNDS-001` 子项提交 `37b524db0601289ccddd129566488cb9ac855e7a`：Render 窗口新增 Entire project / Time selection、整数秒 Tail 和项目采样率/立体声只读提示。时间选区从 tempo-map tick 转到 sample clock；设置在离线 job 入队时冻结，worker 从选区起点 seek，并写入准确范围帧数加尾音。公共 `ProjectWavRenderSettings` 承载导出格式、Master ceiling、起止帧和尾音；项目 schema 未变化，原有全工程/2 秒尾音默认 API 保持兼容。取消或渲染失败仍不会发布临时文件。

验证覆盖非零范围起点、严格输出帧数、项目采样率/立体声 PCM24 header、尾音段、队列快照与 Arrange 后续变化隔离、无选区/空输出禁用、反向范围错误及取消时已有目标文件保留。`cargo fmt --all -- --check`、目标包 Clippy warnings denied、CSV/JSON 校验通过；`xvfb-run -a cargo xtest --no-fail-fast` 为 884/884 passed、0 skipped。证据见[Render 范围记录](../verification/reaper-parity/project-render-range.md)。

此提交不关闭该原子项：当前仍固定立体声、尾音仅整秒；Source、Regions/其他 Bounds、通道选项、Presets/Metadata/Normalize/Limit 及原厂 Render 窗口和尾音语义实测都未完成。`UI-028` 与 `RENDER-BOUNDS-001` 保持 `in_progress`；下一步继续扩展实际导出能力并补足原厂 Render 参照。

## 2026-10-12 Project Render 输出采样率

`RENDER-BOUNDS-001` 子项提交 `31e20f9d556e2bc63dbb36b409a5cf3d772bde12`：Render 可选 Project rate 或 General 已确认的 11 档常用速率。队列捕获具体输出速率；工程图仍以工程时钟渲染，速率不同则由离线 worker 的有状态 192-tap sinc 转换写入 WAV，保持块边界连续，且 WAV header 与输出帧数使用目标速率；同速导出旁路转换，实时 callback 不运行此处理。

回归验证 48→44.1 kHz 的精确 1 秒输出帧数、分块与单块结果一致、声道关系保持，以及非零选择起点和尾音的 WAV 头/数据长度。`xvfb-run -a cargo xtest --no-fail-fast` 全量 885/885 通过；`cargo fmt --all -- --check`、目标 Clippy `-D warnings`、parity CSV/JSON 结构校验和 `git diff --check` 通过。证据见[Render 范围记录](../verification/reaper-parity/project-render-range.md)。

输出声道仍固定立体声，尾音仍仅整秒；当前速率列表来自 General 设置而非原厂 Render 列表实测。Source、Regions/其他 Bounds、Presets、Metadata、Normalize/Limit 与原厂窗口对照未完成，矩阵 `UI-028` / `RENDER-BOUNDS-001` 和 P10 继续 `in_progress`。

### 2026-10-12 Project Render 毫秒精度尾音

Render Tail 输入已从整秒扩展为最多三位小数的秒数；文本先做精确十进制解析，再以最近项目采样帧舍入，不经过浮点。实现提交 `a351b8a13f929dad611166670044aa9243500579`。排队任务冻结毫秒时长，旧全工程渲染入口仍保留两秒默认。48 kHz 工程选择 0.5 秒并追加 1.25 秒尾音后，以 44.1 kHz 输出 WAV，回归检查 77,175 帧的数据区；同时覆盖 `1`、`1.2`、`.5`、`1.` 和非法负数/多余精度输入。`xvfb-run -a cargo xtest --no-fail-fast` 全量 886/886 通过，零跳过；目标 Clippy `-D warnings`、fmt、parity CSV/JSON 校验及 diff 检查通过。毫秒级尾音机制已实现，原厂 Render 输入格式、精度与上限仍需采集；立体声固定、Source/其他 Bounds/Presets 等仍未完成。`UI-028` / `RENDER-BOUNDS-001` 继续 `in_progress`。

### 2026-10-12 Project Render Mono/Stereo

Render channels 对齐提交 `4f87ce3cace6321cd4d11e745c7901321c498a00`：窗口可选 Mono/Stereo；WAV PCM16、PCM24 与 Float32 的头、块对齐、字节数和样本写入均使用所选声道数。Mono 在 Master 立体声总线完成采样率转换后，对左右样本取平均；Stereo 保留左右分离。队列捕获声道数，排队后界面变更不会更改任务。旧导出入口仍默认为 Stereo。

在 Linux REAPER 7.82 / Default 7 / fresh isolated config 下运行[通道探针](../verification/reaper-parity/render-channels.md)：一秒、48 kHz PCM24 stereo 常量素材 L=0.125/R=0.375，经中心 Master mix 输出 Stereo 首帧仍为 `[0.125, 0.375]`，Mono 为 `0.25`，文件各 48,000 帧。探针脚本与输出事实已提交，不保留 REAPER 工程或媒体。AAADAW 回归覆盖三种编码的 Mono header/平均值、范围+尾音单声道 WAV 和队列声道快照。`xvfb-run -a cargo xtest --no-fail-fast` 全量 887/887 通过、零跳过；目标 Clippy `-D warnings`、fmt、66 行矩阵唯一 ID/结构、reference JSON 和 diff 检查通过。

这只闭合 Mono/Stereo 受测子集：多声道输出、非中心声像 downmix、stem/source modes、Track/Item/Region/Razor 以及完整 Render 对话框仍未完成。矩阵新增 `RENDER-CHANNELS-001` 与 `RENDER-SOURCE-001` 并保持 `in_progress`；`RENDER-BOUNDS-001` 也继续开放。

### 2026-10-12 Project Render Peak normalization

`RENDER-POSTPROCESS-001` 子项提交 `17338f8`：Render 窗口加入默认关闭的 `Peak normalize to 0 dBFS`。队列冻结设置；worker 先完成输出采样率转换，再依照 Mono/Stereo 最终输出映射测量 sample peak，用私有临时 spool 和固定大小缓冲区进行第二遍增益处理。正常化仍只发生在离线 worker；取消检查覆盖第二遍及 WAV 发布前，取消/失败会删除 spool 和未完成 WAV，原子目标文件流程不变。

Linux REAPER `7.82/linux-x86_64` / Default 7 fresh config 探针记录：控制源为 48 kHz PCM24 stereo，一秒常量 L=0.125/R=0.375；`RENDER_NORMALIZE=0` 保持原样，启用 sample-peak（flags=5）并设线性 target=0.5 后输出约 `[0.1666665, 0.5]`。官方 ReaScript 说明 `RENDER_NORMALIZE_TARGET` 使用线性目标；原厂 7.81 User Guide §21.3 列有 Peak normalization。证据、受测限制和复现脚本见[峰值归一化记录](../verification/reaper-parity/render-normalize.md)。AAADAW 端到端回归检查经 44.1 kHz SRC 的 Mono PCM24 输出到达满刻度；取消发生在第二遍第一块写入后，验证临时文件清理；队列测试验证已排队 normalization 值保持不变。

最终验证 `xvfb-run -a rtk proxy cargo xtest --no-fail-fast`：888/888 通过、0 跳过；目标 Clippy `-D warnings`、fmt、CSV 67 个唯一 ID、9 个 reference JSON 解析和 `git diff --check` 通过。矩阵新增 `RENDER-POSTPROCESS-001`，与 `RENDER-BOUNDS-001`、`RENDER-CHANNELS-001`、`RENDER-SOURCE-001`、`UI-028` 均保持 `in_progress`。RMS/LUFS/True Peak、可调目标、limiter、其它 postprocess 和完整 Render 对照未完成；下一步扩展其他高频 Render 输出能力，不关闭此原子项。

### 2026-10-12 Project Render Selected track via master

`RENDER-SOURCE-001` 单轨子项提交 `2b2f95c`：Render Source 新增 `Selected track via master`。该选项要求 Arrange 中恰好选择一轨，且将 TrackId 固定到队列任务。离线 worker 从项目快照构造临时 render project，将该轨 solo、清除其他 solo，再沿已有 master 图渲染；不改保存工程、实时播放或原 UI 选择。源轨/文件夹/发送效果路径使用现有引擎 solo-routing，不另做一套离线混音实现。

Linux REAPER 7.82 / Default 7 / fresh isolated config 原生探针把两条相同居中轨道分别以 `RENDER_SETTINGS=0` 与 `128` 渲染：Master mix 与单轨 via-master 文件都是 48 kHz PCM24 stereo、48,000 帧；稳定样本分别是 `[0.25, 0.75]` 和 `[0.125, 0.375]`，单轨文件名 wildcard `$track` 生成 `selected-via-master-Selected.wav`。脚本与受测边界见[Render Source 记录](../verification/reaper-parity/render-source.md)。AAADAW 的双轨集成回归确认其 via-master 输出相对完整 Master mix 是单轨电平，solo 快照单测确认其它 solo 被清除而原项目状态不变；UI 校验无选择/多选时报错，队列快照回归确认 TrackId 不随之后选择更改。

`xvfb-run -a rtk proxy cargo xtest --no-fail-fast` 全量 891/891 通过、0 跳过；目标 Clippy `-D warnings`、fmt、CSV 67 个唯一 ID、9 个 JSON reference 和 `git diff --check` 通过。随后在同版本 REAPER 中运行峰值归一化探针：flags=0 的样本为 `[0.125, 0.375]`，flags=5/target=0.5 为约 `[0.1666665, 0.4999995]`，每个文件 48,000 帧。`RENDER-SOURCE-001` 与 `RENDER-POSTPROCESS-001` 仍为 `in_progress`：多选分文件、其它 Source/Bounds、wildcard 面板、可调归一化目标、RMS/LUFS/True Peak/limiter 尚未实现。下一步继续拓展 Render 的批量/范围能力并采集相应原厂输出，不将单轨子集等同于 Source 对齐完成。

### 2026-10-12 Project Render 可调峰值目标

`RENDER-POSTPROCESS-001` 目标设置子项提交 `8481d77`：峰值归一化目标输入扩展为 −60 至 0 dBFS、百分之一 dB 精度，默认仍为 0 dBFS。文本解析为有界整数百分之一 dB，队列捕获启用状态和目标；worker 以 `10^(dBFS/20)` 转成线性峰值，再沿既有离线双遍 spool 路径处理。正目标被限制以避免超 0 dBFS 的峰值输出；目标范围尚未与原厂 UI 边界逐项对照。

7.82 探针的线性 target=0.5 输出约 `[0.1666665, 0.4999995]`；AAADAW `−6.02 dBFS` 线性换算为约 `0.5000346`，PCM24 回归确认输出对应目标值并保留左右声道比例。回归还覆盖 `0`、`−0.5`、`−6`、`−6.02`、`−60`、精度/范围拒绝，以及排队后修改目标不改变已入队任务。证据更新在[峰值归一化记录](../verification/reaper-parity/render-normalize.md)。

最终 `xvfb-run -a rtk proxy cargo xtest --no-fail-fast` 为 893/893 通过、0 跳过；目标 Clippy `-D warnings`、fmt、CSV 67 个唯一 ID、9 个 JSON reference 和 diff 检查通过。该项仍只实现 Master mix sample-peak 子集；RMS/LUFS/True Peak、limiter、stem normalization、fade/trim/pad 等仍缺。`RENDER-POSTPROCESS-001` 与 `UI-028` 保持 `in_progress`。

### 2026-10-12 Project Render 多选轨道 via-master stems

实现提交 `8d497bd`。`RENDER-SOURCE-001` 的多轨 via-master 子项：Source 允许选择多轨；一个选择仍写入用户指定的单 WAV 路径，多选则按 Arrange 顺序冻结 TrackId 与 `所选文件名-轨道名.wav` 输出路径。轨道名冲突追加序号，路径分隔符和文件系统保留字符先替换为下划线。worker 逐轨复用原有离线 Master 图和 solo 隔离快照，不改保存工程或实时播放。所有 WAV 先写入目标目录同文件系统上的私有暂存目录；目标已存在即拒绝，取消不发布；发布期间发生并发冲突会清理本次已创建的链接并保留并发文件。Android 单文件 Document Picker 禁用多文件提交。

REAPER 7.82 / Linux / Default 7 新鲜配置原厂脚本现同时实测单选和双选 flags=128。双选输出 `selected-stems-Selected.wav` 与 `selected-stems-Unselected.wav`；每个文件均为 48 kHz PCM24 stereo、48,000 帧、288,000 data bytes，首帧约 `[0.125000015, 0.375000045]`，而 Master mix 控制约为 `[0.250000030, 0.750000089]`。对应证据在[Render Source 记录](../verification/reaper-parity/render-source.md)。AAADAW 集成回归使用同一音源置于两轨，确认两 stem 独立样本、准确 WAV 长度；并验证已有文件保护、逐轨中途取消无输出、发布时竞争文件触发回滚、队列选择与命名冻结。

最终 `xvfb-run -a rtk proxy cargo xtest --no-fail-fast` 为 896/896 通过、0 跳过；媒体/app/UI 目标 Clippy `-D warnings`、fmt、67 个唯一 parity ID、9 个 JSON reference 和 `git diff --check` 通过。

多轨 Source 仍不完整：Render wildcard 编辑器与 REAPER 文件名规则、Master+stems、pre-fader/plain stems、folder/send/FX 原厂对照、Items、Regions、Razor 和 Render Matrix 未实现。Android 单文件 picker 不支持多文件输出。`RENDER-SOURCE-001`、`UI-028` 和 P10 继续 `in_progress`。

### 2026-10-12 Project Render Master mix + stems

实现提交 `c601974`。Render Source 新增 `Master mix + stems`，并按原厂顺序显示 Master mix、Master mix + stems、Selected tracks via master。组合模式按用户选中的轨道顺序生成一份 `prefix-Master.wav` 与每轨一份 `prefix-Track.wav`，共同使用既有输出格式、采样率转换、Mono/Stereo 和峰值归一化设置。离线 worker 的输出集合允许混合 MasterMix 与单轨 via-master 源；整组先渲染到私有暂存区，通过 no-clobber 链接发布，取消/运行时发布失败会清理已暂存/已链接文件。Android 单文件 Document Picker 禁用多文件模式。

REAPER 7.82 / Linux / Default 7 原厂探针以两条相同居中轨道、双选和 flags=1 产出 Master、Selected、Unselected 三个 WAV；都为 48 kHz PCM24 stereo、48,000 帧。样本分别为 Master `[0.250000030, 0.750000089]`、两个 stem `[0.125000015, 0.375000045]`。旗标为 1 且渲染前后读回保持；flags=2/129/130 的脚本探针渲染后都回到 0 并仅产出组合 Master 文件，故 plain stems 仍列为未实现，尚需通过原厂 UI 继续调查。细节见[Render Source 验证记录](../verification/reaper-parity/render-source.md)。

AAADAW 集成回归确认 grouped Master+stems 输出每份 WAV 帧数/首样本正确；同一 grouped publisher 同时验证已有目标保护、逐轨取消无文件、并发目标冲突回滚。最终 `xvfb-run -a rtk proxy cargo xtest --no-fail-fast` 为 897/897 通过、0 跳过；媒体/app/UI 目标 Clippy `-D warnings`、fmt、67 个唯一 parity ID、9 个 JSON reference 解析与 `git diff --check` 通过。`RENDER-SOURCE-001`、`UI-028` 和 P10 继续 `in_progress`。

### 2026-10-12 Project Render plain selected-track stems

原厂 UI 探针改为实际打开 Render 窗口，而不是仅从 `GetSetProjectInfo(RENDER_SETTINGS)` 读回推断。两条已选轨道使用同一段 48 kHz PCM24 常量源，Master fader 设置为线性 0.5（−6.0206 dB）。在 Source 选择 `Master mix and stems` 后，Master 与两个轨道 stem 均为 48 kHz PCM24 stereo、48,000 帧、首帧 `[0.125, 0.375]`：Master 混合先将两轨信号相加再经 0.5 fader，stem 则保持单轨电平。Source 选择 `Selected tracks (stems)` 会输出两份独立 WAV，首帧同为 `[0.125, 0.375]`。原来 flags=2 直接 setter 的结果不代表 UI 行为；setter 没有可靠同步 Render source 下拉框。复现脚本和原厂窗口截图已加入 Render Source 验证记录。

AAADAW 增加 `Selected tracks (stems)` 源：离线 worker 对每轨构建 solo 隔离快照，临时 Master gain 置 unity、绕过最终 Master safety clamp，输出仍经过轨道混音与已有轨道 FX；保存工程、播放与 UI 选择保持不变。`Master mix and stems` 改为组合 MasterMix 与普通 stems；`Selected tracks via master` 继续独立经过 Master 输出级。单轨复用指定 WAV 路径，多选沿用同文件系统暂存、无覆盖发布、取消与发布失败清理。受控回归在 Master −6.0206 dB 时确认 Master `[0.125, 0.375]`、plain stem `[0.125, 0.375]`、via-master stem `[0.0625, 0.1875]`，并检查 WAV 帧长与队列源模式。

最终 `xvfb-run -a rtk proxy cargo xtest --no-fail-fast` 为 898/898 通过、0 跳过；媒体/app/UI 目标 Clippy `-D warnings`、fmt、67 个唯一 parity ID、JSON 解析和 `git diff --check` 通过。`RENDER-SOURCE-001`、`UI-028` 与 P10 仍 `in_progress`：REAPER 文件名规则、pre-fader stems、Folder/Send/FX 细节、Region matrix、Items、Razor、多声道以及完整 Render 窗口交互仍未覆盖。

### 2026-10-12 Project Render stems pre-fader

REAPER 原厂窗口勾选 `Render stems pre-fader` 后，轨道 `D_VOL=0.5` 的 stems 从 `[0.0625, 0.1875]` 变为 `[0.125, 0.375]`；两种状态都是两份 48 kHz PCM24 stereo、48,000 帧文件。默认 Source 为 `Selected tracks (stems)`，同一 `[Selected tracks (stems)]` / [checkbox checked](../verification/reaper-parity/images/render-pre-fader-stems-native.png) 截图分别记录模式和开启状态；复现脚本可生成前推子或后推子窗口。

AAADAW Render 面板加入原厂 checkbox，plain stems 和 Master mix and stems 可操作；Master mix 与 via-master 模式禁用该控件。单轨和多轨队列分别捕获 post-fader/pre-fader 源类型，组合渲染只改变每个 stem，Master 输出不变。离线源快照把选中轨道静态音量设为 unity 并清除其音量自动化，Master gain 与最终 safety clamp 按普通 stems 路径绕过；原项目和实时播放保持不变。真实 WAV 回归断言 post-fader 静态/自动化增益输出 `[0.0625, 0.1875]` / `[0.03125, 0.09375]`，pre-fader 两者都输出 `[0.125, 0.375]`；原厂自动化、Folder/Send 路径对比仍未完成。

`xvfb-run -a rtk proxy cargo xtest --no-fail-fast` 为 899/899 通过、0 跳过；媒体/app/UI 目标 Clippy `-D warnings`、fmt、67 个唯一 parity ID、JSON 和本次文档本地链接校验通过。`RENDER-SOURCE-001`、`UI-028` 与 P10 仍 `in_progress`：native volume automation 和路由下的 pre-fader tap 语义尚待采集；Region matrix、Items、Razor、多声道、通道映射、Wildcard 与完整窗口行为继续未覆盖。

### 2026-10-12 Render 线性 WAV 编码格式

Linux REAPER 7.82 Default 7 Render to File 的 WAV 位深菜单列出 PCM 8/16/24/32-bit、IEEE float 32/64-bit，以及 IMA ADPCM、cADPCM、u-Law。对两条相同 48 kHz PCM24 源轨进行 Master mix 实渲染后，PCM8 文件头为 PCM/8-bit/stereo/48 kHz/48,000 帧，首样本无符号字节 `[160, 224]`；PCM32 为 PCM/32-bit、首样本有符号整数 `[536870912, 1610612736]`；Float64 为 IEEE float/64-bit、首样本 `[0.25, 0.75]`。菜单截图和复现脚本见[Render Formats 验证](../verification/reaper-parity/render-formats.md)。

AAADAW 的 Render Format 选择新增全部六种线性 WAV 编码；PCM8 使用 RIFF 要求的 unsigned offset representation，PCM32 使用 4-byte signed PCM，Float64 使用 64-bit IEEE float fmt chunk。Float64 容器提升保存引擎当前的 f32 输出值，不改变 DSP 精度。六种格式都走现有离线图、格式冻结、固定缓冲写入、取消清理和 no-clobber 发布路径。测试已扩展到每种编码的 WAV 头/数据、Mono/Stereo 映射、dither 原子性及同一工程图的离线导出/解码往返。原厂压缩编码和完整格式菜单对照仍未实现，不关闭格式原子项。

矩阵新增 `RENDER-FORMATS-001`；`RENDER-CHANNELS-001` 更新为覆盖六种线性编码。`RENDER-FORMATS-001`、`RENDER-CHANNELS-001`、`UI-028` 与 P10 继续保持 `in_progress`：IMA ADPCM/cADPCM/u-Law、noise shaping、RF64、大文件边界及全量 Render 窗口状态仍待完成。

最终 `xvfb-run -a rtk proxy cargo xtest --no-fail-fast` 为 901/901 通过、0 跳过；媒体/app/UI Clippy `-D warnings`、fmt、68 个唯一 parity ID、reference JSON、本次文档链接及 `git diff --check` 均通过。P10 与三个 Render 原子项继续 `in_progress`，没有将未实现的压缩编码或完整原厂 Render UI 标为完成。

### 2026-10-12 Render 8-bit u-Law

REAPER 7.82 原厂 Render 选择 `8 bit u-Law` 后生成 WAVE format tag 7、8-bit、48 kHz stereo、48,000 帧。常量 Master mix `[0.25, 0.75]` 编码为 `[159, 135]`；另以 16 个 3,000-frame 电平块检查符号、零点与幅度变化，原厂编码 `[0, 7, 15, 31, 47, 53, 68, 99, 255, 227, 196, 181, 175, 159, 143, 135]` 与 G.711 μ-law 测试向量一致。[格式记录](../verification/reaper-parity/render-formats.md)含菜单/对话框截图、哈希与可复现的 source fixture generator。

AAADAW 新增 `8-bit u-Law` 输出格式：使用 WAVE format tag 7 和 18-byte fmt chunk，实现标准分段 companding，Mono 使用已有的左右声道平均，Stereo 保持独立声道；奇数长度的单声道数据会写入 RIFF pad byte。写入仍经过现有固定内存离线队列、取消清理和 no-clobber 发布；u-Law 输出由应用自己的 media decoder 回读验证。应用中的线性 TPDF dither 不应用于 companding 路径，原厂对 u-Law 的全部 dither/noise-shaping 语义仍待对照。

`RENDER-FORMATS-001` 和 `RENDER-CHANNELS-001` 现在覆盖六种线性格式、4-bit IMA ADPCM 和 8-bit u-Law，仍保持 `in_progress`：原厂 2-bit cADPCM、全量格式菜单/dither、RF64 与完整 Render 窗口均未实现或验收。

奇数帧单声道 RIFF 填充修正后，针对性 u-Law/Mono 测试通过；完整 `xvfb-run -a rtk proxy cargo xtest --no-fail-fast` 为 902/902 通过、0 跳过，媒体/app/UI Clippy `-D warnings` 通过。fmt、68 个唯一 parity ID、reference JSON 与 `git diff --check` 也通过；Render 覆盖仍按上列缺口保持 `in_progress`。

### 2026-10-12 Render 4-bit IMA ADPCM

Linux REAPER 7.82 Default 7 的 `4 bit IMA ADPCM` 原生 Render 输出为 WAVE tag 17、48 kHz stereo、4-bit、block align 2048、samples per block 2041；`fmt` chunk 为 20 bytes，`fact` chunk 的帧数为 48,000，数据为 24 个完整块（49,152 bytes）。菜单及已选择编码的截图和原生文件/数据 SHA-256 见[Render 格式记录](../verification/reaper-parity/render-formats.md)，可复现的 16 段 PCM24 输入由共享向量脚本生成。

AAADAW 新增有界块式 IMA encoder、tag 17/fmt/fact/data 写入、4-bit 左低右高 nibble layout 和最后一个块的 fact 帧数。启用 Symphonia ADPCM decoder 后，媒体层读取 RIFF `fact` 计数并截掉最后一个 ADPCM block 的填充帧；否则 `48,000` 帧原生输出会被块长度误报为 `48,984` 帧。writer 回归校验 mono/stereo block headers、PCM level predictors、16 个电平锚点、解码帧数、取消和 no-clobber。

IMA 输出的头字段、原生 block predictor 与解码电平锚点已对照；REAPER 与 AAADAW 的有损 nibble 在若干阶跃过渡处仍非逐字节一致，精确 encoder/dither 对照继续开放。cADPCM、BWF metadata/完整格式窗口在后续切片继续推进，因此 `RENDER-FORMATS-001`、`RENDER-CHANNELS-001`、`UI-028` 与 P10 均保持 `in_progress`。

完整 `xvfb-run -a rtk proxy cargo xtest --no-fail-fast` 回归为 905/905 通过、0 跳过；媒体/app/UI strict Clippy `-D warnings`、`cargo fmt --all -- --check` 与 `git diff --check` 通过。此提交只扩大格式支持，不关闭 Render parity 条目。

### 2026-10-12 Render 2-bit cADPCM

REAPER 7.82 Default 7 原厂 WAV 使用私有 format tag `0xAC0C`，stereo block align 2048、每块 4081 帧、2-bit 样本和 `fact` 总帧数。原厂常量源文件解码回 `[0.25, 0.75]`；另保存了带 BWF 元数据的 16 电平×3,000 帧原厂向量文件及所选格式窗口截图。该参考文件 SHA-256 为 `745b4788710dcdc25639838457738e8ae1f502f86bf8eb3d5b762657fe5dc60b`，格式字段、block predictors、数据哈希和电平锚点见[Render 格式记录](../verification/reaper-parity/render-formats.md)。

AAADAW 现在用共享的有界 ADPCM block writer 导出 cADPCM，并通过自有媒体路径解析原厂 BWF/cADPCM、按 fact 帧数流式解码。原厂向量的 12 个 block predictor 全部匹配，解码的 16 个电平中心均在 0.001 内，当前编码 payload 至少 99% 字节与原厂一致。其余不一致集中在量化器阶跃边界，故仍未达 bit-exact；此限制继续列入 `RENDER-FORMATS-001` 验收条件。Mono/Stereo 编码、格式冻结、取消/no-clobber 和九种输出格式的离线图回归纳入验证，Render 全部界面、BWF 写入、RF64、其它 bounds/格式语义仍未关闭。

当前提交前验证：workspace nextest 907/907 通过、0 跳过；workspace all-targets Clippy `-D warnings` 与 `cargo check --workspace --all-targets` 通过。改用 boxed worker decoder 后，媒体 WAV 解码集成、cADPCM 原厂向量与九格式离线渲染专项也在最终代码上通过。`RENDER-FORMATS-001`、`RENDER-CHANNELS-001`、`UI-028` 与 P10 保持 `in_progress`。

### 2026-10-12 Project Markers 基础切片

依据 Linux REAPER 7.82 默认 Main action list 的 40157/40172/40173，实现 Project Marker 稳定 ID、Project/DawAction 事件、撤销/重做、快照、SQLite schema 35 迁移与往返。Insert 菜单和 `M` 在编辑光标加标记；`[`/`]` 严格导航到前后标记，并在没有前项时回项目起点、没有后项时落到可推导的 Item 末尾。Arrange 标尺显示旗标/名称，主时间线画布也将竖线覆盖全部轨道。原厂探针实测 action 40613 删除光标前后严格小于 1 秒的最近 Marker，光标重合时可删除，正好 1 秒不删除；等距时删除较早位置者，Region 不参与候选。新增 40656–40665 编号 Marker 动作：按位置顺序移动第 N 个标记并保留名称，N 不存在时在光标插入空标记，Region 不计数；默认键为 `Ctrl+1`…`Ctrl+9` 和 `Ctrl+0`（第 10 个）。当前仅在停止播放时使用编辑光标，播放中的 play cursor 行为与实时安全集成仍待验证。

Marker 输入和状态回归覆盖名称/位置校验、稳定排序、Undo/Redo、快照、schema34→35、默认键、导航和 GPU 绘制数据。实现遇到旧存储测试通过降低 `user_version` 模拟旧库的方式；将 schema35 表及索引创建改为 `IF NOT EXISTS` 后，schema34 缺表迁移和历史降级夹具均通过。基础 Marker 插入/导航切片的 workspace nextest 为 914/914、0 跳过；近光标删除扩展后为 915/915；加入编号 Marker 槽位、主键盘映射、NumPad 不冲突和稳定身份 Undo 后，workspace nextest 917/917、0 跳过。strict workspace Clippy、`cargo check --workspace --all-targets` 与 fmt 检查通过；原厂证据分别见 `marker-delete-near-cursor-native.tsv` 和 `numbered-markers-native.tsv`。

`PROJECT-MARKERS-001` 和 UI-013 继续 `in_progress`：Marker 双击命名/管理器、Region、颜色/隐藏、负时间、tempo/timebase 行为及原厂实窗视觉/边界对照未完成。`[`/`]` 的末尾行为目前按 AAADAW 可见 Item/Marker 推导，也还没有证明覆盖 REAPER 所有项目结束规则；不据此关闭 P4 或 UI-013。

### 2026-10-12 Marker 标尺选择与拖动

在 Linux REAPER 7.82 Default 7 的一次性工程中实测：点击 Marker 旗标会定位编辑光标；拖动旗标移动 Marker，单次 Undo 恢复整个拖动；Snap/Grid Settings 默认 Snap 开启、Arrange 网格 `1/4`，开启时标记吸附到四分音符网格，关闭时可停在网格线之间。原厂位置输出、操作结果与截图见[鼠标交互证据](../verification/reaper-parity/fixtures/marker-ruler-native.tsv)。

AAADAW 标尺现在支持 Marker 旗标/标签单击选择和编辑光标定位；拖动显示游标预览，保持鼠标抓取偏移，释放后以一条 `UpdateProjectMarker` 历史动作提交，Undo/Redo 保留 Marker ID 和名称。提交经过播放/工程忙碌守卫。Arrange 默认网格从 `1/16` 改为原厂实测 `1/4`；MIDI Editor 网格分度独立控制（AAADAW 当前默认为 `1/16`，原厂 MIDI Editor 的初始分度还未实测），并共用 Snap 开关，匹配原厂默认未勾选的“Use same grid division”选项。尚未对照双击命名、鼠标修饰键、同位置 Marker 命中/排序、负时间及全网格设置；不要据此关闭 `PROJECT-MARKERS-001`、P4 或 UI-013。

专项回归：`ruler_marker_click_selects_marker_and_moves_edit_cursor`、`ruler_marker_drag_preserves_grab_offset_and_publishes_one_move`、`ruler_marker_drag_uses_enabled_quarter_note_snap_grid`、`ruler_marker_move_is_one_undoable_edit_and_respects_busy_guard`。最终 workspace nextest 921/921、0 跳过（串行运行；CLAP X11 helper 测试串行复验通过）；workspace all-targets strict Clippy `-D warnings`、`cargo check --workspace --all-targets`、fmt 与 `git diff --check` 均通过。`PROJECT-MARKERS-001` 仍为 `in_progress`。

### 2026-10-12 Marker 双击改名

REAPER 7.82 Default 7 原厂探针确认：双击 Marker 打开 Edit Marker，Name 字段默认获得焦点；OK 接受名称，Cancel 丢弃草稿，Ctrl+Z 一步恢复原名。原厂窗口和项目文件结果见[Marker 编辑证据](../verification/reaper-parity/fixtures/marker-edit-native.tsv)及[原厂 Edit Marker 截图](../verification/reaper-parity/fixtures/marker-edit-dialog-native.png)。

AAADAW 现从标尺双击进入 Edit Marker 窗口并聚焦名称输入；键入后按 Enter 或点 OK，通过 `DawAction::UpdateProjectMarker` 提交一次可撤销名称修改；Cancel、Escape 或关闭窗口会丢弃草稿。第二次点击若开始拖动仍按拖动处理，不误开编辑窗。原厂 ID、Lane、Position、颜色和 Marker Manager 尚未实现；AAADAW 窗口暂只编辑名称，不能视为完整属性或管理器对齐。

聚焦 nextest 中 Marker editor Undo/Redo/取消/非法输入与忙碌守卫、双击命中/位置容差及“第二击转拖动”共 9 项全部通过。Linux 图形会话实窗验证双击打开、名称输入自动获得焦点、直接键入并 Enter 保存、点击 Cancel 不保存。原厂和 AAADAW 窗口截图见[原厂窗口](../verification/reaper-parity/fixtures/marker-edit-dialog-native.png)与[AAADAW 窗口](../verification/reaper-parity/fixtures/marker-edit-dialog-aaadaw.png)，取消后的工程画面见[AAADAW 取消结果](../verification/reaper-parity/fixtures/marker-edit-cancel-aaadaw.png)。`PROJECT-MARKERS-001`、P4 和 UI-013 仍保持 `in_progress`；后续优先扩展原厂 Marker Manager 与属性字段，并完成同位置、Region 和时间基准行为。

### 2026-10-12 Region/Marker Manager 基础列表

Linux REAPER 7.82 Default 7 的 action 40326 和默认 `Ctrl+Alt+Shift+R` 已核对。原厂管理器初始启用 Regions、关闭 Markers；Markers 开启后可见 `#`、Name、Start、End、Length、Current、Visible、Ruler Lane 等列。单击行选择并定位编辑光标，双击单元格可编辑；完整原厂窗口和行为见[Manager 证据记录](../verification/reaper-parity/project-markers.md)。

AAADAW 新增独立 Region/Marker Manager 窗口、View 菜单动作与默认快捷键。它沿用原厂初始过滤状态；Marker 列支持按名称、显示编号和 bar.beat.hundredths 搜索，行选择移动编辑光标，Edit 调用现有名称窗，Delete 使用稳定 Marker ID 历史动作。管理器仍显示明确的 Region 未实现状态，未把 Region checkbox 当成已具备 Region 数据能力。实窗验证改名后列表即时刷新，并在管理器保持焦点时用 Ctrl+Z 撤销；截图见[默认状态](../verification/reaper-parity/fixtures/marker-manager-default-aaadaw.png)、[Marker 列表](../verification/reaper-parity/fixtures/marker-manager-list-aaadaw.png)和[编辑窗](../verification/reaper-parity/fixtures/marker-manager-edit-dialog-aaadaw.png)。

本切片没有实现原厂 End、Length、Visible、Ruler Lane、Render Matrix、Lane Manager、Options、列排序/拖动、双击单元格内编辑、完整 Marker 属性或 Region。`PROJECT-MARKERS-001`、UI-013 和 P4 继续 `in_progress`；下一优先项是补齐 Edit Marker 的位置属性并继续调查 ID/Lane 字段与其时间格式语义。

专项测试覆盖默认过滤、名称/编号/位置筛选、行选择定位、编辑入口、删除/Undo、忙碌保护和全局 Undo 快捷键。最终 workspace nextest 930/930 通过、0 跳过；workspace all-target strict Clippy `-D warnings`、`cargo check --workspace --all-targets`、`cargo build -p aaadaw`、fmt、69 个唯一 parity ID、证据路径和 `git diff --check` 通过。GUI 证据是基础功能验收，完整 REAPER 复刻仍未完成。

### 2026-10-12 Edit Marker Position 属性

Linux REAPER 7.82 Default 7 原厂 Edit Marker 显示 Name、ID、Lane、Position 和 Marker Manager 入口。当前默认 Bars/Beats 显示下，Position 接收 `bar.beat.hundredths`；将 `3.1.50` 改为 `3.2.00` 会移动 Marker，编辑光标仍停在原位置；Cancel 丢弃 Position 草稿。字段、Marker/光标分离和 Cancel 结果见[原厂属性窗](../verification/reaper-parity/fixtures/marker-edit-position-native.png)、[时间线](../verification/reaper-parity/fixtures/marker-edit-position-cursor-native.png)和[Cancel](../verification/reaper-parity/fixtures/marker-edit-position-cancel-native.png)。这轮没有独立确认原厂 Position 修改的 Undo 粒度。

AAADAW 在 Edit Marker 增加 ID 只读显示、默认 Bars/Beats Position 编辑、Marker Manager 入口及明确的 Lane 未实现状态。Position 解析/格式化沿用 Item Properties beats/timebase 路径；Name 和 Position 一起通过一条 `UpdateProjectMarker` 提交，保留稳定 ID，Undo/Redo 一步恢复名称与 ticks。Cancel/Escape/关闭丢弃草稿，非法位置保留窗口，播放/工程忙碌时仍拒绝提交。实窗以 120 BPM 验证 `1.2.50` 将标记从 0 秒移动到 0.75 秒，编辑光标保持在 0 秒，Ctrl+Z 恢复起点；截图见[属性窗](../verification/reaper-parity/fixtures/marker-edit-position-aaadaw.png)、[位置提交](../verification/reaper-parity/fixtures/marker-edit-position-applied-aaadaw.png)和[Undo](../verification/reaper-parity/fixtures/marker-edit-position-undo-aaadaw.png)。

Targeted Marker Properties nextest 2/2 通过；最终 workspace nextest 930/930 通过、0 跳过，workspace all-target strict Clippy `-D warnings`、`cargo check --workspace --all-targets`、`cargo build -p aaadaw`、fmt、69 个唯一 parity ID、Project Markers 证据路径/本地链接与 `git diff --check` 均通过。打开 bundled example 的集成测试现在先复制到临时目录，避免 SQLite 初始化改写仓库里的示例工程。原厂 ID 编辑、Lane 数据/选择、位置负时间与其它显示模式、舍入边界和原厂 Undo 粒度仍未完成，颜色属性也尚未实现。`PROJECT-MARKERS-001`、UI-013 和 P4 继续 `in_progress`，下一个 Marker 切片转向可编辑 ID 与 Lane 的模型/持久化前先验证原厂 ID 重排及 Lane 语义。

### 2026-10-12 Project Marker 可见编号与稳定身份

扩大原厂编号探针后发现，旧探针仅凭名称和位置变化得出“编号槽位按位置排序”的结论不充分。REAPER 7.82 Default 7 的动作 40657 实际按可编辑 Marker number 查找：编号可稀疏、可重复；缺少目标编号时新增该编号，重复时选择时间位置最早的匹配项；普通 Insert 在已有 5、10 时自动新增编号 1。完整复测记录和脚本见[编号身份 TSV](../verification/reaper-parity/fixtures/project-marker-id-slots-native.tsv)与[探针](../../scripts/reaper_parity/probe-project-marker-id-slots.lua)。本节更正基础 Marker 记录里旧的按位置结论。

AAADAW 现在将内部稳定 `MarkerId` 与可编辑可见编号分开，新增显式编号插入和一次性 Name/ID/Position 属性更新；编号动作按可见编号查找，重复编号选最早位置，缺号时创建目标编号，普通插入填最小正编号。管理器、空名称标尺显示和筛选均改读可见编号。SQLite schema 36 保存编号；schema 35 升级按旧版稳定 ID 还原此前显示值，并保留 Marker 身份、名称和位置；同时迁移会检查列是否存在，以兼容通过降版 `user_version` 构造的历史夹具。Lane 的原厂字段是否可写及其保存语义仍待确认，未加入未验证的 Lane 数据模型。

验证：原生编号身份探针通过；core Marker、schema 35 迁移、稀疏/重复编号 SQLite 往返、属性编辑、重复编号动作和无效 ID 测试通过。第一次并行全量回归发现 schema36 迁移未覆盖“新列已存在但版本号被降级”的测试夹具，修复为列检查后，`xvfb-run -a rtk cargo nextest run --workspace --no-fail-fast -j 1` 为 933/933 通过、0 跳过；workspace all-target Clippy `-D warnings`、`cargo check --workspace --all-targets`、`cargo build -p aaadaw`、fmt 和 `git diff --check` 通过。`PROJECT-MARKERS-001`、UI-013、P4 保持 `in_progress`；Region、Manager 其余列/行内编辑、Lane/颜色、负时间与其它 timebase、播放中 play cursor、tempo/meter 行为和完整视觉验收仍未完成。下一步继续填补 Marker Manager 原厂列/功能覆盖。

### 2026-10-12 Project Region 基础模型与 Manager 编辑

依据 Linux REAPER 7.82 默认 Main action list，确认从 Time Selection 插入 Region 的 action `40174` 默认键 `Shift+R`；编辑插入 `40306`、光标附近删除/编辑 `40615`/`40616` 和 Region/Marker Manager `40326` 已列入原厂动作目录。本切片没有把未实测的 Region 编号、自动分配、重叠边界或颜色规则当作原厂事实。

新增稳定 `RegionId`、可见编号/Name/Start/End、DawAction 历史、快照及 SQLite schema 37；非空 Time Selection 可通过 Insert 菜单/Shift+R 创建 Region，Arrange 显示范围和边线，Region/Marker Manager 可筛选、选中并设置 Time Selection/编辑光标、打开属性窗编辑 Name/ID/Start/End，或删除并撤销。无效或忙碌时编辑不写入项目。Region 独立加入 `PROJECT-REGIONS-001`，自动编号、Ruler 点击/拖动、循环联动、Render Matrix、负时间/其它显示单位、原厂工程格式和完整 Default 7 几何仍未验收。

最终代码下，串行 workspace nextest 为 942/942（66 个测试二进制、0 跳过）；workspace all-target Clippy `-D warnings`、`cargo check --workspace --all-targets`、`cargo build -p aaadaw`、fmt、`git diff --check` 通过。`PROJECT-REGIONS-001`、`PROJECT-MARKERS-001`、UI-013 和 P4 均保持 `in_progress`。下一步扩大可操作性：补齐 Region 标尺命中/选择与移动/边界编辑，再逐项探测编号和 loop 语义。

### 2026-10-12 Region 标尺选择、移动与边界编辑

Region 标尺现在可以在色带内单击设置整个 Time Selection 和编辑光标，在色带内拖动整段并保持长度，在标尺下部拖动 Start/End 边线分别改动一个边界。复用 Arrange Snap/Grid；画布显示拖动预览，释放后提交一条稳定 RegionId 的 `UpdateProjectRegion`，保留编号和名称并支持 Undo/Redo。播放/录音/工程忙碌守卫拒绝提交，不向音频 callback 发送编辑状态。

加入点击选区、整段移动、Start/End 两侧边界调整、网格吸附、单步撤销、忙碌拒绝和过期拖动冲突回归。最终 workspace nextest 950/950（66 个测试二进制、0 跳过），workspace all-target strict Clippy、`cargo check --workspace --all-targets`、`cargo build -p aaadaw`、fmt 和 `git diff --check` 通过。此手势映射与范围吸附还未在 Linux REAPER 7.82 实窗对照，Region hit priority、重叠边界、鼠标修饰键、Lane/颜色、循环/Repeat 和 Region Render Matrix 仍未验收；`PROJECT-REGIONS-001`、`PROJECT-MARKERS-001`、UI-013 和 P4 保持 `in_progress`。下一项优先探测并补齐可操作的 Region 导航与循环联动。

### 2026-10-13 REAPER Region 光标动作 40615/40616

在 Linux REAPER 7.82 Default 7 隔离工程中复测 Delete near cursor 的左闭右开边界、空隙、Marker/Time Selection 排除和重叠规则；结果是最新 Start 优先、同 Start 时短区间优先、完全相同时较早枚举项优先。Edit near cursor 打开包含 Name/ID/Lane/Position/Length、颜色操作及 Manager/Render Matrix 入口的原厂窗口且不移动编辑光标。原始数据、截图和复现脚本已归档至 `docs/verification/reaper-parity/fixtures/` 与 `scripts/reaper_parity/`。

AAADAW action list 新增对应的 Delete/Edit region near cursor 命令。Delete 复用稳定 RegionId 的 `DawAction`，Undo 可恢复，Marker 保持不变；Edit 跳转到当前 Region 属性草稿。两者仅在编辑光标包含于 Region 且工程空闲时启用。仍需实现原厂 Lane、独立 Length、颜色、Region Manager 和 Render Matrix 控件，不能把编辑动作映射视作窗口对齐完成。`DISPLAY=:101 rtk cargo nextest run --workspace --no-fail-fast -j 1` 为 952/952 通过、0 跳过（66 个测试二进制）；strict Clippy `-D warnings`、workspace all-target check、`cargo build -p aaadaw`、fmt 与 `git diff --check` 通过。PROJECT-REGIONS-001、PROJECT-MARKERS-001、UI-013 和 P4 仍为 `in_progress`。

### 2026-10-13 Loop points 与 Time Selection 链接动作

Linux REAPER 7.82 Default 7 的隔离工程探针确认默认 Loop points 链接开启；40621 开启时以 Time Selection 覆盖 Loop points，关闭时保留范围；未链接的 40622/40623 分别把 Time Selection 复制到 Loop points、把 Loop points 复制到 Time Selection。40624 只在未链接时清除 Loop points；40634/40635 链接时清除两种范围，40020（Escape）无论链接状态都清除两种范围。另验证 40222/40223 的 `{`/`}` 默认键、无范围时创建端点的方式、跨越另一端时区间折叠到新端点，以及链接与分离时对 Time Selection 的影响。未链接时原厂画面中的 Loop bracket/手柄与独立 Time Selection 色带见[Default 7 截图](../verification/reaper-parity/fixtures/loop-time-selection-unlinked-native.png)。用例及可复现脚本见[链接/清除 TSV](../verification/reaper-parity/fixtures/loop-time-selection-linkage-native.tsv)、[端点 TSV](../verification/reaper-parity/fixtures/loop-point-edit-native.tsv)及对应[链接探针](../../scripts/reaper_parity/probe-loop-time-selection-linkage.lua)、[端点探针](../../scripts/reaper_parity/probe-loop-point-edit.lua)。

AAADAW 新增独立 `LoopPoints`（可表达零长度）、默认开启的链接状态，以及 40222/40223、40621、40622、40623、40624、40634、40635、40020 的 Action List 入口；40222/40223 默认绑定 `{`/`}`，Escape 清除两种范围。现有时间选区事件和端点动作在链接时同步两种范围，分离后可独立更新，清除与复制语义按原厂探针处理。该实现只涉及 UI 时间线状态，没有将范围推送至实时音频 callback。首次 workspace nextest 发现 factory 默认 Escape 被用户快捷键验证器当作可配置绑定拒绝，影响既有快捷键/Action List 测试；现仅允许内置 40020 默认值通过，显式自定义普通 Escape 仍按全局取消规则拒绝，Action List 保留 Escape 标签。针对修复后的 `loop_points_and_time_selection_follow_reaper_link_copy_and_clear_actions` 为 1 passed；`DISPLAY=:101 rtk cargo nextest run --workspace --no-fail-fast -j 1` 为 953/953 passed（66 个测试二进制）。`cargo check --workspace --all-targets`、workspace strict Clippy `-D warnings`、`cargo build -p aaadaw`、fmt 与 `git diff --check` 均通过。新增矩阵项 `PROJECT-LOOP-001` 继续保持 `in_progress`：loop 专用标尺绘制/拖动、Repeat 播放、偏好持久化、工程保存恢复、Tempo/Meter/timebase 变化、Action List 链接勾选状态和完整 REAPER 实窗交互/视觉还没有完成。

### 2026-10-13 Loop points 标尺括线与端点拖动

Linux REAPER 7.82 Default 7 原厂实测：未链接时，拖动 Loop start/end 不改变独立 Time Selection；start 从 3 秒移至 2.5 秒、end 从 4 秒移至 4.5 秒的逐阶段结果见[端点拖动记录](../verification/reaper-parity/fixtures/loop-point-drag-native.tsv)和[可复现探针](../../scripts/reaper_parity/probe-loop-point-drag.lua)。

AAADAW Ruler 底栏现在绘制灰色 Loop bracket 与两端手柄。拖动端点时在画布预览，释放后一次性提交 LoopPoints；默认链接开启时由现有状态规则同步 Time Selection。实窗画面见[AAADAW 截图](../verification/reaper-parity/fixtures/loop-time-selection-aaadaw.png)，其中保留未链接的 Time Selection `[1,2]` 和 Loop `[3,4]`。此截图窗口尺寸和缩放与原厂不同，只验证显示形态与两种范围分离，不代表完整窗口几何对齐。

本切片增加手柄命中/底栏约束、交越折叠和 Canvas 预览/提交回归。最终 workspace nextest 为 955/955 通过、0 跳过（66 个测试二进制）；`cargo check --workspace --all-targets`、workspace strict Clippy `-D warnings`、`cargo build -p aaadaw`、fmt 检查和 `git diff --check` 通过。`PROJECT-LOOP-001` 保持 `in_progress`：空白底栏建区、整段移动、鼠标修饰键及 Snap 边界、Action List 链接勾选状态、Repeat 播放、偏好/工程持久化、Tempo/Meter/timebase 变化和完整原厂交互/视觉仍未完成；未将 Loop points 引入实时音频 callback。

### 2026-10-13 Loop Ruler 整段拖动与零长度点手势

Linux REAPER 7.82 / Default 7 隔离探针确认：普通拖动已有 loop bracket 主体或空白底栏，都将 Loop 折叠为按下位置的零长度点；普通点击空白底栏只定位编辑光标。Shift+拖动 bracket 主体保留长度并移动整段；Snap 开启时依默认四分音符网格吸附。未链接时 Time Selection 保持独立，链接时底栏建点同步两种范围。逐阶段 Loop/Time Selection/Snap/编辑光标值见[原厂 TSV](../verification/reaper-parity/fixtures/loop-point-ruler-gestures-native.tsv)，手动阶段探针见[Lua 脚本](../../scripts/reaper_parity/probe-loop-point-ruler-gestures.lua)。

AAADAW runtime modifier 状态现在传入 Timeline；Ruler 的 Shift+主体拖动有长度保持预览，释放一次提交并沿用现有 Snap/链接规则。普通底栏拖动在鼠标按下处建立零长度点；未拖动时只将编辑光标定位于点击处。原厂实测未发现该 lane 单次建非零范围的手势，AAADAW 不虚构该行为。本实现只更新 UI 工程状态，不将范围发送给实时音频 callback。

新增 Canvas 回归覆盖 Shift 整段移动、Snap 后长度保持、主体/空白底栏建点、空白点击定位以及 runtime 修饰键同步；Loop 手势专项当前 8/8 通过。最终 `xvfb-run -a rtk cargo nextest run --workspace --no-fail-fast -j 1` 为 960/960 passed、0 skipped（66 个测试二进制）；聚焦 Loop 手势为 8/8。`cargo check --workspace --all-targets`、workspace strict Clippy `-D warnings`、`cargo build -p aaadaw`、fmt 与 `git diff --check` 均通过；parity CSV 为 71 个唯一 ID、15 列。`PROJECT-LOOP-001`、P4 和 P6 仍为 `in_progress`：Repeat 与 Transport/audio-engine 联动、配置/工程持久化、其它修饰键、端点与空白点创建的 Snap/边界、链接 Action List 勾选状态、Tempo/Meter/timebase 和完整原厂视觉仍未完成。

### 2026-10-13 Loop points 与 Time Selection 工程持久化

Linux REAPER 7.82 / Default 7 原厂探针通过 `Main_SaveProjectEx` 保存两个隔离 `.rpp`，再启动独立实例并以 `GetSet_LoopTimeRange2` 回读：Time Selection 与 Loop points 分别恢复 `SELECTION`/`SELECTION2` 秒数范围，既覆盖未链接的独立 `[1.25,2.5]` / `[3.5,5.75]`，也覆盖保存时 linked 且范围相同的情况。40621 link 状态在重开后跟随隔离 REAPER 配置 `locklooptotime`，不是工程文件字段；原始结果和探针见[持久化 TSV](../verification/reaper-parity/fixtures/loop-time-selection-persistence-native.tsv)与[ReaScript](../../scripts/reaper_parity/probe-loop-persistence.lua)。

AAADAW 新增 SQLite schema 38，以工程采样原点的秒数原子保存两种范围，Loop 可保存零长度；加载时用该工程采样率和 tempo map 转回当前 timeline tick。时间范围编辑纳入未保存修订，project load 恢复保存状态；link 开关仍作为全局配置缺口，不会错误地混入工程范围表。schema 37 项目迁移为空范围，无效范围回滚整个工程保存。全 workspace nightly+Cranelift nextest 在调试符号关闭的测试构建下为 961/961 passed、0 skipped；stable `cargo check --workspace --all-targets`、stable strict Clippy `-D warnings`、fmt 和 `git diff --check` 通过。Stable 默认调试符号测试链接在本环境超过 30 分钟并耗尽 overlay 空间，清理可重建的包构建缓存后改由上述 nightly 全量测试完成；stable 静态检查仍覆盖全部 targets。PROJECT-LOOP-001、P4 和 P6 继续 `in_progress`：Repeat/实时循环、link 偏好持久化、负时间与其它 timebase、Snap/修饰键/边界和 Default 7 完整视觉仍未验收。

### 2026-10-13 Loop 40621 全局链接偏好持久化

原厂验证表明 40621 随 REAPER 用户配置 `locklooptotime` 恢复，而不属于 `.rpp` 工程。AAADAW 将链接布尔值存入平台全局用户配置目录的 `loop-time-preferences.json`，缺失配置默认链接开启；启动时恢复，切换后 500 ms 防抖原子保存，主窗口关闭请求及关闭事件都会刷新待写值。此设置不会单独增加工程未保存修订；如果切换时按原厂语义同步了实际范围，范围变化仍由既有工程修订逻辑处理。设置文件使用版本 1，未知版本拒绝加载且不覆盖原文件。

回归覆盖配置默认值、未链接 round-trip、未知版本保留，以及 40621 切换后安排全局写入且不误标工程修改。全 workspace nightly+Cranelift nextest 为 963/963 passed、0 skipped（66 个测试二进制）；Stable `cargo check --workspace --all-targets`、strict Clippy `-D warnings`、fmt 与 `git diff --check` 通过。Cranelift 构建仍报告此前已记录的 iced/wgpu `TimelinePipeline: MaybeSync` 递归深度 warning；Stable Clippy 无警告。PROJECT-LOOP-001、P4 和 P6 保持 `in_progress`：Repeat 播放及实时边界安全、其它修饰键/Snap/边界、Action List 40621 勾选状态、Tempo/Meter/timebase 和完整 Default 7 交互/视觉尚未验收。

### 2026-10-13 Action List 40621 链接状态反馈

REAPER 7.82 Action List 的 `State` 列显示 toggle action 当前 `on`/`off`。AAADAW 已有同列及其他 toggle action 状态映射；补充 `loop-time.toggle-link` 到当前 `TimelineState.loop_points_linked` 的映射后，40621 行会反映当前链接值。回归通过真实 Action List command dispatch 分别断言默认 `on` 与执行后的 `off`，同时保留上一切片对全局偏好防抖保存、工程修订不变的断言。

Loop 链接状态专项 1/1 通过；映射后 workspace nightly+Cranelift nextest 为 963/963 passed、0 skipped（66 个测试二进制），Stable all-target check、strict Clippy `-D warnings`、fmt、71 项/15 列 parity CSV 与 `git diff --check` 均通过。Cranelift 保留 iced/wgpu `TimelinePipeline: MaybeSync` 既有递归深度 warning，Stable Clippy 无警告。PROJECT-LOOP-001、P4 和 P6 仍为 `in_progress`：Repeat 播放安全、其它修饰键/Snap/边界、Tempo/Meter/timebase 和完整 Default 7 交互/视觉尚未验收。

### 2026-10-13 Loop 起止导航动作 40632/40633

动作目录确认 REAPER 7.82 Main section 有 `Go to start of loop` 40632 和 `Go to end of loop` 40633。隔离 Default 7 ReaScript 从 4 秒光标开始验证：无 Loop points 时两项均保持 4 秒；Loop 为 `[1,2]` 秒时分别移至 1 秒与 2 秒。原始值见[Loop 导航 TSV](../verification/reaper-parity/fixtures/loop-navigation-native.tsv)，探针见[脚本](../../scripts/reaper_parity/probe-loop-navigation.lua)。AAADAW 将二者加入 Action List；有范围时定位到起点/终点，无范围时保持编辑光标不变，不改 Loop 范围或工程修订。播放期间导航行为仍待实测。

专项 `loop_cursor_actions_jump_to_existing_boundaries_and_noop_without_a_loop` 已通过，覆盖范围存在/缺失和无工程修改；原厂已验证停止状态下无范围保持光标不变、有范围时定位对应端点。nightly+Cranelift workspace nextest 为 964/964 passed、0 skipped（66 个测试二进制），Stable all-target check、strict Clippy `-D warnings`、71 项/15 列 parity CSV 和 diff 检查通过。Cranelift 显示既有 iced/wgpu 递归深度 warning，Stable Clippy 无警告。PROJECT-LOOP-001、P4 和 P6 继续 `in_progress`；播放期间导航及安全的 Repeat 播放仍待验证/实现。

### 2026-10-13 主窗口文本焦点与全局快捷键分流

Linux REAPER 7.82 / Default 7 原厂界面验证轨道名称编辑焦点：Space 和普通字符进入名称框且不启动播放；Ctrl+Z 穿透并撤销最近插入的轨道；Escape 关闭名称编辑器且保留标尺范围。逐步结果与三张原厂截图见[焦点验收记录](../verification/reaper-parity/input-focus.md)；40696/40001 的动作名和编号来自固定版导出。

AAADAW 已按控件事件状态分流文本与全局按键。本切片增加 `focused_track_name_editor_routes_native_text_escape_and_undo_keys`，覆盖被输入控件消费的 Space、Escape 的草稿取消及范围保留、未消费 Ctrl+Z 对应全局 Undo。nightly+Cranelift 定向 nextest 1/1 passed；stable `cargo clippy -p aaadaw --all-targets -- -D warnings`、fmt、71 项/15 列矩阵、证据链接和 `git diff --check` 通过。该结果只覆盖 Arrange 轨道名称编辑；Action List/脚本编辑、其它文本字段、AltGr/键盘布局、插件和子窗口焦点仍待逐项核验，INPUT-FOCUS-001 继续 `in_progress`。

### 2026-10-13 REAPER 主菜单访问键

Linux REAPER 7.82 / Default 7 键盘探针确认 F10 不打开菜单；Enter 随后仍执行 Play。原厂 `Alt+F/E/V/I/O/A/H` 分别打开 File/Edit/View/Insert/Options/Actions/Help，同一 `Alt+F` 再次按下仍保持 File 菜单打开；Alt+T/M 在该配置下未打开菜单。原厂逐项结果和七张菜单截图见[Menu keyboard 验收记录](../verification/reaper-parity/menu-navigation.md)。

AAADAW 已移除不匹配的 F10 菜单行为，为当前已有的 File/Edit/View/Insert/Actions 菜单接入经原厂验证的 Alt+F/E/V/I/A，并让同一访问键重新聚焦菜单。新增 `menu_keyboard_uses_native_alt_mnemonics_and_accepts_enter_while_actions_search_has_focus`，与既有箭头/命令选择回归合计定向 nextest 2/2 passed；stable aaadaw strict Clippy、fmt 和 diff 检查通过。新增 `SHELL-MENU-KEYBOARD-001` 后矩阵为 72 项/15 列，仍保持 `in_progress`：AAADAW 尚缺 Options/Help 两个菜单，完整九项导航、Item/Track 命令和所有焦点例外仍未验收。

### 2026-10-13 Options 菜单：Preferences 与 Loop link

将现有 Preferences 窗口从 File 移到原厂 Options 菜单，显示原厂 `Preferences...` 与 `Ctrl+P`；保留原有 `file.settings` 稳定动作 ID，避免丢失已保存的快捷键绑定。把已有的 40621 loop-link 功能加入 Options，按 `loop_points_linked` 显示勾选态；执行后沿用全局 `loop-time-preferences.json` 防抖持久化，不误增工程修订。主菜单增加 Options 与 `Alt+O`，左右切换顺序现包含 File/Edit/View/Insert/Item/Track/Options/Actions。

新增回归覆盖 Options 命令和勾选状态变更、Alt+O、菜单栏项及左右导航；nightly+Cranelift（独立 `target/cranelift`、测试调试符号关闭）定向 nextest 为 5/5 passed、496 skipped。Stable `cargo clippy -p aaadaw --all-targets -- -D warnings`、fmt、`git diff --check` 及 72 项/15 列 parity CSV 唯一 ID/证据路径检查通过。`SHELL-MENU-KEYBOARD-001` 继续 `in_progress`：Help 和 `Alt+H`、九项完整菜单顺序、Options 其余大量功能、Item/Track 命令及原厂全菜单视觉/焦点行为仍未完成。

### 2026-10-13 Help 菜单与 About

新增第九项 Help 和原厂 `Alt+H`；Help 的 Action list and shortcuts 项复用现有 Action List，About AAADAW 打开单实例应用信息窗口，`Ctrl+F1` 也打开 About。About 展示应用名称、当前版本和项目地址；窗口可由关闭按钮或系统关闭事件清理，不影响工程状态。菜单顺序现在是 File/Edit/View/Insert/Item/Track/Options/Actions/Help。

Help/About 及 Options、九项菜单访问键和左右导航定向 nightly+Cranelift nextest 为 6/6 passed、496 skipped；stable all-target strict Clippy、fmt、diff 与 parity CSV 检查通过。`SHELL-MENU-KEYBOARD-001` 仍 `in_progress`：Help 原厂 Documentation、键鼠绑定、HTML Action List、ReaScript、EULA、购买信息、changelog 和更新检查缺失；Options 其余功能、Item/Track 内容、所有状态/禁用项和完整原厂 GUI 验收也未完成。

### 2026-10-13 Action List 重复快捷键

针对 REAPER 7.82 实测中 Undo 与 Show undo history 同时保留 `Ctrl+Alt+7` 的结果，AAADAW 现在允许不同普通动作保留相同快捷键。Action List 提交和 Settings 捕获都经过同一配置校验；Undo 显式设为 `Mod+S` 后，Save project 的工厂默认值仍显示为 `Mod+S`，按键解析优先执行显式 Undo。不同普通动作的显式映射也会同时保存在配置中。宏与普通动作、多个宏之间的快捷键冲突仍拒绝；Escape 保留规则、未知 action ID、非法键和保存失败的原子性没有放宽。

6 项 nightly+Cranelift 定向回归通过，覆盖 Action List 持久化、Settings 捕获、显式/工厂映射重复、普通动作重复、旧键重叠规则和宏冲突；随后 `aaadaw` 全部 502 项测试通过、0 跳过。Stable `cargo clippy -p aaadaw --all-targets -- -D warnings`、fmt、diff 检查及 parity CSV/证据链接检查也通过。`INPUT-BIND-001` 仍 `in_progress`：实测仅证明显式 Undo 在一个默认动作重复例子中的优先关系；所有重复绑定的 Section/宏/用户动作/工厂默认优先级、Main 以外 Section、默认动作完整覆盖、特殊键/Scope/MIDI/OSC、导入导出与原厂界面验收仍未完成。

### 2026-10-14 Main action 40697：依焦点移除对象

Linux REAPER 7.82 / Default 7 的 Main 动作导出中，40697 `Remove items/tracks/envelope points (depending on focus)` 默认绑定 Delete；40006 `Item: Remove items` 无默认绑定。AAADAW 已将 Delete 从 Item 菜单命令移到稳定 Action List ID `edit.delete-focused`。实现按当前时间线选区处理 Audio/MIDI items、FX/音量 automation 点或轨道；多选轨道先按文件夹/输出/发送依赖顺序删除，并合并为一个 Undo 事务。测试还验证有 item 与轨道同时选中时 item 优先，Undo 恢复数据。

本次应用全量 nightly+Cranelift nextest 551/551 通过、0 跳过；Stable all-target strict Clippy、fmt 与 diff 检查通过。提交后又在独立 Xvfb 对照截取两边 701×475 Action List 窗口：[REAPER](../verification/reaper-parity/fixtures/delete-by-focus-action-list-native.png)、[AAADAW](../verification/reaper-parity/fixtures/delete-by-focus-action-list-aaadaw.png)。画面确认两边主动作描述与 Delete 绑定一致，也显示 REAPER 另有尚未收录的 no-prompting 变体；这不是运行时焦点路由或像素验收。`INPUT-BIND-001` 仍 `in_progress`：原厂不同窗口的焦点优先级尚未探测；AAADAW 当前按选择类别路由，还没有把实际键盘焦点窗口纳入动作分派。Main 外 Section、完整动作目录、特殊键/Scope/MIDI/OSC、导入导出和完整 GUI 验收也仍缺。

### 2026-10-14 Main action 40697：no-prompting 变体

隔离 REAPER 7.82 Action List 截图还列出 `Remove items/tracks/envelope points (depending on focus) - no prompting`，虽然当前默认动作 TSV 没有独立 action ID。AAADAW 已将其作为 `edit.delete-focused-no-prompting` 加入 Main Action List，不配置默认键；它与主动作共用同一 DeleteByFocus 编辑事务。最新[REAPER 参照](../verification/reaper-parity/fixtures/delete-by-focus-action-list-native.png)与[AAADAW](../verification/reaper-parity/fixtures/delete-by-focus-no-prompt-aaadaw.png)截图中，两边均列有这两个动作，窗口客户区均为 701×475。AAADAW 删除路径当前不显示确认框，因此两个入口执行结果相同；默认 REAPER 动作是否会因对象类型/偏好弹出确认尚未实测，不能声称 no-prompting 语义已经对齐。

加入该入口后应用全量 nightly+Cranelift nextest 再次为 551/551 通过、0 跳过，Stable all-target strict Clippy、fmt 与 diff 检查通过。Action List 覆盖、原厂确认规则及不同窗口的焦点优先级仍未验收；`INPUT-BIND-001` 保持 `in_progress`。

### 2026-10-13 Main Action 40182：Select all items

默认动作目录给出 40182 `Item: Select all items`，快捷键导出为空。AAADAW 注册为稳定 action ID `item.select-all`；从 Action List 搜索/执行后按轨道和时间选择全部 Audio 与 MIDI items，并同步活动 item/轨道锚点，清除 automation point 选择。它不发送 Project/DSP 动作，也不改变工程修订或 Undo/Redo；无 items 时禁用。加入相邻 40421 动作后，完整 `aaadaw` nightly+Cranelift nextest 为 504/504 通过、0 跳过；Stable strict Clippy、fmt 与 diff 检查通过。矩阵新增 `ITEM-SELECT-001` 且保持 `in_progress`：原厂完整运行时边界和实窗焦点验收未做；Main 仅注册这一小部分原厂动作，Item action 覆盖仍远未完成。

同一 Main Section 的 40421 `Item: Select all items in track`（无默认快捷键）也已注册为 `item.select-all-in-track`。当前执行范围是活动所选轨道，混合选中该轨道的 Audio 与 MIDI items 并替换先前多轨选择，仍然不修改工程或历史。`select_all_items_in_track_action_keeps_other_tracks_unselected` 通过 Action List Run 覆盖该路径。完整 `aaadaw` nightly+Cranelift nextest 为 504/504 通过、0 跳过；Stable strict Clippy、fmt 与 diff 检查通过。新增 `ITEM-SELECT-002` 保持 `in_progress`，等待原厂验证多轨选择和 active-track 边界。

### 2026-10-13 Main Actions 40717/40718：时间选区 item 选择

提交 `d5f3d1a` 注册 40717 `Item: Select all items in current time selection` 与 40718 `Item: Select all items on selected tracks in current time selection`，两者均无默认快捷键，可经 Action List 搜索和执行。Audio item 的采样区间先映射到项目 tick，MIDI item 使用其项目 tick；当前以半开区间相交筛选，再按轨道/时间顺序替换选择。40718 使用所有选中轨道及活动轨道。该 UI 状态操作不改变工程修订或 Undo/Redo 历史；缺少时间选区或选区内没有 item 时禁用。

两个 Action List Run 回归覆盖 Audio/MIDI 混合、时间范围内外、起止边界、多轨过滤、活动轨道及工程历史不变；更新了通用全选搜索结果断言。`aaadaw` nightly+Cranelift nextest 为 506/506 passed、0 skipped；Stable `cargo clippy -p aaadaw --all-targets -- -D warnings`、fmt、diff 和矩阵 76 项/15 列唯一 ID 及新行证据路径检查通过。Cranelift 编译显示既有 iced/wgpu `TimelinePipeline: MaybeSync` recursion-depth warning，Stable Clippy 无警告。新增 `ITEM-SELECT-003/004` 均保持 `in_progress`：原厂端点相交、多轨选择运行时边界和实窗界面行为尚未探测；Item/Main 全量动作和整套 REAPER 复刻均未完成。

### 2026-10-13 Main action 40289：Unselect all items

提交 `6b6b53b` 注册 REAPER Main action 40289 `Item: Unselect (clear selection of) all items`，无默认快捷键。Action List Run 清除 item 集合、active item 与 item selection anchor；保留轨道/时间/automation point 选择，不写入工程或 Undo/Redo。没有 item 选择时禁用。点击 Arrange 空白处的既有 item 清除路径也改为复用同一 timeline helper。

Action List Run 回归验证 Audio/MIDI item selection 清空、时间范围/轨道选择保留和工程历史不变；timeline 单元测试覆盖 automation point 选择保留。`aaadaw` nightly+Cranelift nextest 为 508/508 passed、0 skipped；Stable all-target strict Clippy、fmt、diff 和矩阵 77 项/15 列唯一 ID/新增证据路径检查通过。Cranelift 显示既有 iced/wgpu `TimelinePipeline: MaybeSync` recursion-depth warning，Stable Clippy 无警告。`ITEM-SELECT-005` 继续 `in_progress`：原厂动作启用条件和对 automation point 选择的影响尚未探测，完整 Item actions 与 100% REAPER 对齐仍未完成。

### 2026-10-13 Main action 40528：Select item under mouse cursor

提交 `942c3c5` 为 Arrange 增加 item hover hit state，并注册 Main action 40528 `Item: Select item under mouse cursor`，默认无快捷键。Timeline shader 的鼠标移动/离开事件更新或清除 target；工程重建、行布局变化、平移和缩放会清掉 hover，以免操作使用旧几何位置，之后需鼠标再次移动命中。Action List Run 选择当前 target 并替换 item selection；不改变工程/Undo。无有效 target 时禁用。

`arrange_cursor_motion_tracks_and_clears_the_hovered_item` 覆盖真实 shader 指针事件命中与清除；`select_item_under_mouse_action_uses_current_hover_and_replaces_selection` 覆盖 Action List 搜索/Run、替换选择和工程历史不变。`aaadaw` nightly+Cranelift nextest 为 510/510 passed、0 skipped；Stable all-target strict Clippy、fmt、diff 和矩阵 78 项/15 列唯一 ID/证据路径检查通过。Cranelift 有既有 iced/wgpu `TimelinePipeline: MaybeSync` recursion-depth warning；Stable Clippy 无警告。`ITEM-SELECT-006` 保持 `in_progress`：REAPER 原厂 Action List 焦点、启用状态、实窗指针 target 和视图变换后重新命中行为尚未实测。

### 2026-10-13 Main actions 40529/40530：鼠标下 item 添加与切换选择

提交 `682c199` 注册 40529 `Item: Select item under mouse cursor (leaving other items selected)` 与 40530 `Item: Toggle selection of item under mouse cursor`，均无默认快捷键。它们复用同一 Arrangement hover target：40529 永远保持目标选中并保留已有选择，40530 对目标增删选择；二者不改工程/Undo。该提交时按预期目标启用状态处理；2026-10-16 的原厂 Action List 实测发现无目标时 Run 仍可用，后续修正见下文。

`select_item_under_mouse_leaving_other_items_selected_adds_without_toggling` 覆盖两次添加仍不取消已有/目标项；`toggle_selection_of_item_under_mouse_adds_and_removes_the_hovered_item` 覆盖移除和重新加入。完整 `aaadaw` nightly+Cranelift nextest 为 512/512 passed、0 skipped；Stable all-target strict Clippy、fmt、diff 和矩阵 80 项/15 列唯一 ID/证据路径检查通过。Cranelift 显示既有 iced/wgpu recursion-depth warning；Stable Clippy 无警告。`ITEM-SELECT-007/008` 保持 `in_progress`：多选后的原厂 active item/anchor 及 Action List 焦点/hover 边界尚未探测；100% 复刻仍未完成。

### 2026-10-13 Main action 41115：Invert selection

动作目录确认 Main 41115 `Item: Invert selection` 无默认快捷键。AAADAW 注册 `item.invert-selection`，Action List Run 反转整个工程的 Audio/MIDI item 选择集合。隔离 REAPER 7.82 / Default 7 探针确认空选择选中全部、部分选择取补集、全选清空，光标和轨道选择不变；每次选择改变均增加工程状态计数并建立 `Invert item selection` Undo 条目。AAADAW 在应用级 Undo/Redo 中保存选择状态，内容修订与 Project 内容 Undo 栈不变。

`invert_item_selection_action_selects_the_complement_of_audio_and_midi_items` 现在覆盖空/全/部分选择、Action List Run、撤销和重做。原厂 Action List 可用状态、活动 item 规则及实窗焦点仍待测，`ITEM-SELECT-009` 保持 `in_progress`；原厂逐项数据与探针见 `docs/verification/reaper-parity/fixtures/item-invert-selection-native.tsv` 和 `scripts/reaper_parity/probe-item-invert-selection.lua`。

### 2026-10-13 Main actions 41127/41128：相邻非重叠 item 选择

官方 7.82 / Default 7 的真实点击探针确认 41127/41128 只在边界精确相接时选中同轨目标；有间隙时无目标。多选逐项映射，映射不到的源项会从结果移除，全部无目标时清空选择，光标不动。实现现在将 Audio sample 与 MIDI tick 转换到共同项目 tick，比对精确边界，并按原厂清除/多选行为替换旧的“允许间隙、单项锚点”假设。

`adjacent_non_overlapping_item_actions_navigate_within_the_active_track` 和 `adjacent_item_actions_require_touching_boundaries_and_map_each_selection` 覆盖精确边界、跨媒体类型、多选逐项映射、无目标清空、Action List Run 及项目/Undo 不变；原厂点击记录见 [item-navigation fixture](../verification/reaper-parity/fixtures/item-navigation-native.tsv)。`ITEM-SELECT-010/011` 保持 `in_progress`：空选择的动作可用性、Action List 键盘焦点、完整实窗布局与全部 item-selection 交互仍未验收。

### 2026-10-13 Main actions 41167/41168：移动光标到最近 item 边缘

动作目录确认 41167/41168 左右最近 item edge 导航无默认快捷键。原厂运行探针确认范围为整个工程的 Audio 与 MIDI items，即使没有 item 选择也可运行；等于当前光标的边缘会跳过。AAADAW 比较连续项目 sample 位置，Audio 起止边界不再先量化为整数 tick，MIDI 边缘仍使用 tick；只更新时间线 edit cursor、保留 item selection，不修改工程历史。方向上没有边缘时禁用。

`item_edge_navigation_actions_move_to_nearest_audio_or_midi_boundary` 使用非 tick 对齐 Audio 边界，覆盖混合边缘、无选择、方向无目标、相等端点、选择保留及工程/Undo 不变；原厂结果见 [item-navigation fixture](../verification/reaper-parity/fixtures/item-navigation-native.tsv)。REAPER 原厂 off-grid 光标精度、AAADAW 当前分数 tick 光标实窗捕获、完整 Action List 焦点和 Arrange 实窗外观还未验收，`ITEM-SELECT-012/013` 保持 `in_progress`。

### 2026-10-13 Main actions 41139/41140：相邻轨道 item 导航及原厂边界修正

隔离官方 Linux REAPER 7.82 / Default 7 / Dummy Audio 探针实测 41139/41140：只检查项目顺序紧邻的上一/下一轨；源为首个按轨道和时间排列的 selected item；目标为该轨上第一个与源时间区间正长度重叠的 item。成功时只选择目标、光标移至目标起点并保持轨道选择；无重叠目标时清空 item 选择、不跳过空轨且不移动光标。探针通过真实 item 点击驱动，避免仅用 API 修改选择时丢失原厂活动 item 状态。

AAADAW 新增稳定动作 `item.select-previous-track` / `item.select-next-track`。同时按原厂输出修正 41127/41128 的严格边界、多选映射及无目标清空行为。`adjacent_track_navigation_uses_first_overlapping_item_and_preserves_track_selection` 与相邻选择回归覆盖紧邻空轨、多个重叠候选、多选锚点、光标、轨道选择、边界和 Undo；native TSV 与复现脚本已落档。Stable `aaadaw` 全量测试 496/496 通过，all-target strict Clippy `-D warnings`、fmt、diff 检查及 parity CSV 87 项/15 列结构和证据路径检查通过。本切片的 nightly+Cranelift 冷构建未产出可运行测试二进制，因此不计作通过；隔离 target 已清理。当前仅覆盖动作核心语义；空选择可用性、Action List 键盘焦点、实窗视觉和整套 Item 行为仍待验收，`ITEM-SELECT-010` 至 `ITEM-SELECT-015` 均保持 `in_progress`。

### 2026-10-13 Main actions 41173/41174：移动光标到所选 items 边界

REAPER 7.82 / Default 7 原厂点击探针确认 41173 将 edit cursor 移到所选 Audio/MIDI items 的最早起点，41174 移到最晚终点；两项覆盖同轨/跨轨多选，空选择不移动光标，选择和工程状态保持不变。记录追加到 [item navigation TSV](../verification/reaper-parity/fixtures/item-navigation-native.tsv)，可用已存探针脚本复测。

AAADAW 注册 `item.navigate-cursor-selected-start` 与 `item.navigate-cursor-selected-end`，按共同项目 tick 计算所选区间的 min-start/max-end，只修改 edit cursor。`selected_item_cursor_actions_use_outer_audio_midi_bounds` 经 Action List Run 验证跨轨 Audio/MIDI 选择、忽略未选的更早 item、保留 item/track selection、空选择 no-op 以及项目/Undo/Redo 状态不变。Stable `aaadaw` 全量测试 497/497 通过，strict all-target Clippy `-D warnings`、fmt、diff 和 parity CSV 89 项/15 列唯一 ID/证据路径检查通过。当前 edit cursor 使用整数 tick，任意 Audio sample endpoint 可能被量化；非 tick 对齐样本位置、空选择时原厂 Action List 启用状态、完整键盘焦点与 Arrange 实窗视觉仍待确认，`ITEM-SELECT-016/017` 保持 `in_progress`。

### 2026-10-13 Main actions 41182/41183/41184：所选 items 网格对齐

REAPER 7.82 / Default 7 点击探针确认 41182、41183、41184 分别将每个所选 Audio/MIDI item 独立对齐到左侧、右侧和最近网格点；最近点正中选择左侧。已在网格上的 item 不移动；Snap toggle 关闭时动作仍按项目网格工作；空选择没有变化。混合多选长度、选择和光标保留，单次 Undo 恢复所有 item。逐项记录及复测入口在 [item snap fixture](../verification/reaper-parity/fixtures/item-snap-native.tsv) 和 [probe](../../scripts/reaper_parity/probe-item-snap.lua)。

AAADAW 新增稳定 Action List ID `item.snap-left`、`item.snap-right`、`item.snap-nearest`。MIDI 使用 PPQ 网格点；Audio 先将网格 tick 映射至项目 sample，再按精确 sample 边界决定方向和最近点；所有移动组成单个 `BatchTransaction`，保留 item 时长及 Audio source offset。三个回归覆盖 Action List 搜索/Run、Audio/MIDI 分别对齐、最近点平局、跨媒体多选单步 Undo/Redo、Snap toggle 关闭、精确网格 no-op、空选择和 project-edit busy 守卫。

Stable `aaadaw` 全量测试 500/500 通过，strict all-target Clippy `-D warnings`、fmt、diff 和 parity CSV 92 项/15 列唯一 ID/新增证据路径检查通过。此原厂采样使用四分音符直网格；Swing 网格、播放期间原厂动作可用性与完整 Action List/Arrange 视觉和焦点尚未验收，`ITEM-SELECT-018/019/020` 保持 `in_progress`。AAADAW 目前遵循 `project_edit_busy` 在 playback active 时阻止这些工程编辑，须由后续原厂探针确认这项安全守卫与默认行为的差异范围。

#### 2026-10-16：Swing 网格及 Snap/Grid Settings

REAPER 7.82 / Default 7 探针覆盖直网格、25%、50%、100%、−50% swing，Snap 开关两种状态、九个起点、Audio/MIDI 与三项 item snap action，共 540 行。奇数网格点按 swing 百分比的一半偏移，偶数网格点保持直网格，最近点平局向左；41182/41183/41184 无视全局 Snap 开关。实测非零小节起点不是动作的有效目标点。完整数值、可重放 ReaScript、二进制/数据哈希和配置后的 Settings 截图见 [原厂 swing 验证](../verification/reaper-parity/action-list-and-input.md#2026-10-16-main-actions-411824118341184swing-网格复测)。

AAADAW 已实现 Swing 网格 toggle、−100% 至 100% amount、独立 Snap/Grid Settings 窗、Options 菜单入口、Alt+L 和 42304 Action List 动作；三项 item snap 现在按 signed swing grid 计算，沿用 sample-accurate Audio 比较、单事务 Undo 和 playback busy guard。原厂数值点对应回归已加入。

目前 Swing 候选点和 MIDI item 起点均以 1/200 PPQ 子刻度精确表示；该分母覆盖整数网格间隔与整数 Swing 百分比产生的目标位置。精度已贯穿 project action/Undo、SQLite schema 56、Arrangement 几何、实时 MIDI 采样调度和 SMF 输出；定向回归核对了 964.8 与 955.2 PPQ。普通 Arrange 鼠标吸附、MIDI editor Swing 线与音符移动仍需 REAPER 7.82 实测；Action List/Arrange 全窗截图和焦点、播放期间 native availability 与跨会话设置行为也仍未验收。因此 `ITEM-SELECT-018/019/020` 继续 `in_progress`，不能视为完成。

新桌面 binary 已在隔离 Xorg :112 实际启动；Alt+L 打开独立设置窗，真实 XTest pointer/button 切换 Swing checkbox 后画面状态更新，Arrange toolbar 入口也在实窗可见。binary SHA-256、窗口配置及三张 AAADAW 截图见 [Snap/Grid Settings window fixture](../verification/reaper-parity/fixtures/item-snap-grid-settings-aaadaw.tsv)。截图仅确认当前新增窗口和 toolbar 可运行，未作为 REAPER 整体窗口/像素对照验收。

#### 2026-10-16：Swing 网格传播到常规编辑吸附

Arrangement item 拖动和 trim、时间选区端点、编辑光标、loop/region 边界、自动化点，以及 MIDI Editor 的吸附式插入/移动和 Swing 网格线现在共用整数分子运算的 Swing 候选点算法。它按网格序号偏移奇数点、保留偶数点，平局选择左侧，Snap 关闭或 Shift 暂时绕过时保持原位置；MIDI note 群组移动以锚定音符的目标起点吸附，使选区内相对间距不变。相关 timeline 与 MIDI Editor 回归覆盖正/负 swing、边界拖动及可见网格与吸附点一致。

nightly Cranelift 下 `aaadaw` binary tests 通过 684/684；Stable all-target Clippy `-D warnings`、`cargo fmt --all -- --check` 和 `git diff --check` 通过。普通鼠标和 MIDI Editor Swing 语义尚未对照原厂，分数 tick MIDI 起点、播放状态与全窗口焦点/像素对照仍开放；因此三项 snap action 的矩阵状态保持 `in_progress`。

#### 2026-10-16：Snap/Grid/Swing 设置按项目持久化

工程 schema 升至 55，在 `arrangement_view_meta` 保存全局 Snap 开关、Arrangement 网格、MIDI Editor 网格、Swing 开关及 −100% 至 100% amount。旧 schema 工程迁移后采用当前默认值（Snap on、Arrangement 1/4、MIDI Editor 1/16、Swing off、50%），已有自动化轨道、范围与 Repeat 状态保持。设置从项目视图状态恢复；UI 和 Action List 修改经过同一视图事件路径更新工程 revision，并进入正常保存和未保存工程恢复快照。

Storage 往返与 schema 54 → 55 默认值迁移由 `arrangement_view_state_round_trips_and_survives_project_only_saves` 和 `schema_54_projects_gain_default_snap_and_swing_settings` 覆盖；Timeline 项目替换验证全部设置恢复和无视图状态项目的默认值；真实项目 session 保存/重开回归覆盖 Snap off、八分网格、MIDI 三十二分网格及 37% Swing；未保存工程恢复回归确认 Swing 修改推进 revision 并排入恢复快照。Xvfb 下 nightly Cranelift workspace nextest 全量 1290/1290 通过；改正 lint 后 4 个存储、Timeline 和 session 定向回归再次通过。Stable workspace all-target check、strict Clippy、fmt 与 diff 检查通过。最新 Cranelift desktop binary 在 Xvfb :96 实际启动，渲染的 Arrange toolbar 截图与 binary hash 见 [桌面 smoke fixture](../verification/reaper-parity/fixtures/item-snap-grid-desktop-smoke.tsv) 和[截图](../verification/reaper-parity/fixtures/item-snap-grid-desktop-smoke.png)。截图仅验证启动画面与可见 Snap/Grid 控件；项目设置保存/重开仍由状态持久化回归验证。

设置持久化实现不关闭 `ITEM-SELECT-018/019/020`：REAPER 原厂跨会话保存行为尚未单独探测；常规 Arrange 与 MIDI Editor swing 鼠标行为对照、播放期间 Action List 可用性、全窗口视觉/焦点对照仍未完成。

#### 2026-10-16：原厂 MIDI item 分数 PPQ 定位

在 REAPER 7.82 / Default 7 的 120 BPM、4/4 工程网格下追加 90 个 MIDI item 原厂动作结果：41182/41183/41184、Snap 开/关及正负整数百分比 swing。+1% Swing 的 41183 将 0.5 秒 item 起点移动到 0.5025 秒，即 964.8 PPQ；−1% Swing 的最近动作将其移到 0.4975 秒，即 955.2 PPQ，均保留 item 长度和选择。参照脚本与原厂数据见 [sub-tick probe](../../scripts/reaper_parity/probe-item-snap-subtick.lua) 和 [native TSV](../verification/reaper-parity/fixtures/item-snap-subtick-native.tsv)。

之前 540 行原厂探针的 25%、50%、100% 数值在四分音符网格上恰好产生整数 PPQ，不能据此排除分数位置。新的原厂结果确认 REAPER 使用分数项目 PPQ；AAADAW 随后增加精确 MIDI item 位置类型和 `SetMidiItemPosition` action，Swing 候选按整数子刻度运算，不再把 +1%/−1% 目标舍入到整 tick。工程 schema 56 持久化子刻度，旧 schema 55 项目默认导入为整 tick；回放事件映射到连续采样位置，SMF 导出在所选输出 PPQ 才舍入，Arrangement 绘制也保留起止位置的小数部分。新增回归分别确认核心事件映射、Undo/Redo、schema 55 迁移、SQLite 往返、964.8/955.2 候选、实际播放采样点和高分辨率 SMF PPQ。此项代码链路已贯通，但原厂逐项交互截图、常规 Arrange/MIDI Editor mouse 对照、播放期间 Action List 可用性和全窗口视觉/焦点仍未验收；`ITEM-SELECT-018/019/020` 保持 `in_progress`。

#### 2026-10-16：MIDI item fractional PPQ 模型与回放/导出

`MidiItemPosition` 使用 200 个子刻度表示每个项目 PPQ tick；已观测到的 Swing 目标 964.8 和 955.2 均可无损保存。`SetMidiItemPosition` 进入工程事件历史，schema 56 在 `items.start_tick_subtick` 存储位置；schema 55 迁移保持旧项目为整 tick。核心的 MIDI event iterator 提供精确项目位置，实时 schedule 以连续 PPQ 转 sample 后才落到可播放 sample，SMF export 保留小数直到换算到用户选定的 MIDI 文件 PPQ。Arrangement item geometry 使用精确起止位置。

针对性验证包含 `fractional_midi_item_positions_survive_moves_and_event_mapping`（含复制位置保留）、`schema_55_midi_items_migrate_with_zero_fractional_ppq`、`midi_loop_metadata_round_trips_through_snapshots_and_sqlite`、`midi_item_snap_retains_reaper_fractional_swing_ppq`、`event_plan_preserves_fractional_ppq_item_positions_in_playback_samples`、`fractional_midi_item_bounds_survive_timeline_geometry_cache` 和 `midi_export_keeps_fractional_item_position_until_output_ppq_rounding`；完整 MIDI export native byte regressions 通过 27/27。Xvfb 下 nightly Cranelift workspace suite 已通过，Stable strict all-target Clippy `-D warnings` 通过；补充的复制与几何回归也通过。仍需建立从 REAPER 原厂 Action 到 AAADAW 实窗项目位置的直接图像对照；此缺口不改变三条 snap matrix row 的 `in_progress` 状态。

#### 2026-10-17：MIDI Item Properties 精确保留 fractional PPQ

将分数起点贯通到 MIDI Item Properties 的位置显示、长度终点换算和 Apply：编辑 Position 时把 Sample/Time/Beats/H:M:S:F 转为 1/200 PPQ，位置与其它字段通过同一事务提交；长度仍按模型支持的整数 tick 存储，Apply/Undo/Redo 保留起点子刻度。Samples 属性往返测试覆盖 3840.8 PPQ，App 级测试通过属性窗口把 3840.8 移到 3860.8 PPQ 并验证一次 revision 与 Undo/Redo。MIDI Editor 网格拍/小节着色现在在起点带 fractional PPQ 时不再误标整 tick 边界。

nightly + Cranelift AAADAW binary suite 688/688 通过；Stable `cargo clippy -p aaadaw --all-targets -- -D warnings`、`cargo fmt --all -- --check` 与 `git diff --check` 通过。REAPER/AAADAW fractional-position 实窗截图对照及属性窗口完整视觉/焦点验收仍未完成，`ITEM-PROPERTIES-001`、`ITEM-PROPERTIES-UNITS-001` 和 `ITEM-SELECT-018/019/020` 保持 `in_progress`。

#### 2026-10-17：MIDI Position Nudge 与 Inspector 不舍入子刻度

Position Nudge 现在从精确 MIDI PPQ 起点换算连续 sample、应用整数 sample 位移后再恢复 1/200-PPQ 子刻度；所选 MIDI 的 Nudge anchor 和末端 sample 同样使用精确起止位置。混合 Audio/MIDI 的 1 ms Nudge、fractional item 的 480-sample Nudge 与 Undo 均有 App 回归。Arrangement Inspector 将 1/200 PPQ 格式化为最多三位小数，例如 `964.8`、`964.995`，不通过浮点字符串舍入。

nightly + Cranelift AAADAW binary suite 690/690 通过；Stable aaadaw all-target Clippy `-D warnings`、fmt 与 diff 检查通过。Xvfb 中以 964.8 PPQ MIDI fixture 打开最新桌面二进制并选中 item，AAADAW 实窗截图确认 Inspector 显示 `Tick 964.8 · length 3840`；[截图和 binary/project hashes](../verification/reaper-parity/fixtures/midi-item-fractional-inspector-aaadaw.tsv)。这张图仅验证 AAADAW 自身渲染，尚未与 REAPER 原厂窗口做像素或逐项截图对比。Item Nudge 的更多 targets/units 及完整原厂交互仍未验收，`ITEM-NUDGE-SET-001` 和相关选择/属性行维持 `in_progress`。


#### 2026-10-17：Arrangement GPU 几何保留 MIDI fractional PPQ

修复 Timeline 的 GPU item rectangles 丢失 1/200 PPQ 子刻度的问题：静态 item、选择/移动预览现在将精确起止子刻度作为独立小数顶点属性交给 WGSL，参与相对视口起点的像素换算；普通整 tick 图元和淡化线保持零分数。新增实例回归断言 964.8 PPQ 起点和同样带 0.8 子刻度的末端都进入 GPU 数据。重建后的 WGPU/GL 桌面二进制在 Linux Xvfb 1280×800 实际启动并选中 MIDI item；[实窗截图](../verification/reaper-parity/fixtures/midi-item-fractional-inspector-aaadaw.png)与 binary/project/screenshot hashes 见对应 TSV。它证明当前 AAADAW 窗口和 shader 管线可用，并显示 Inspector 的 `Tick 964.8 · length 3840`；尚不构成 REAPER 对照或亚像素位置验收。

最终 tuple 参数调整后的 nightly + Cranelift AAADAW binary suite 通过 691/691；Stable all-target Clippy `-D warnings`、rustfmt 和 diff 检查通过。`ITEM-PROPERTIES-001` 与相关 Position/Nudge 行仍是 `in_progress`，原厂分数位置桌面对照及更完整交互范围仍待验证。

### 2026-10-13 Render Bounds：All project regions 批量输出

依照 P10，本切片实现了 Render Bounds 的 `All project regions`：每个工程区域按各自采样边界生成 WAV，采用所选文件名作为前缀、区域名作为文件名部分，并对清理后重名的区域追加稳定序号。多文件输出作为一个离线批次执行，复用私有暂存目录、目标不覆盖和失败回滚；Android 暂不开放多文件边界。没有区域时明确提示且禁止提交。原厂 REAPER 7.82 / Default 7 实窗确认该边界菜单项会输出三个区域文件，实际文件均为 48 kHz、PCM24、立体声、48000 帧；UI 截图、输出测量、探针和逐项对照见 [render bounds verification](../verification/reaper-parity/project-render-range.md)。

`render_all_regions_freezes_each_region_range_and_output_name` 覆盖逐区域冻结范围、重名文件名、批次参数与无区域错误；`grouped_render_jobs_keep_each_region_range_and_publish_the_complete_set` 覆盖不同范围的输出帧数和完整批次发布。新 UI 测试与 generic offline batch 测试通过，workspace nextest 为 987 passed、1 skipped；跳过项 `selected_track_stems_stage_as_a_set_and_publish_only_after_all_tracks_finish` 在此环境执行时停滞于媒体驱动的 via-Master 渲染，已记录为未解决验证限制，不能据此视为该音频路径验收完成。Stable workspace strict Clippy、fmt 与 diff 检查通过。`RENDER-BOUNDS-001` 仍为 `in_progress`：目前只实现 All project regions；Custom time range、Selected regions、All project markers、Selected markers、原厂输出命名/编码完整映射及 UI 焦点/禁用状态仍待实现和验收。

### 2026-10-13 Render Bounds：All project markers 批量输出

在上一个区域批次 worker 上加入 `All project markers`：按时间排序的 marker 逐个冻结 `[marker, next marker)` 范围，最后一段到最后一个媒体 item 的项目终点；每个 marker 作为单个输出文件，可组合现有 Render source，且整个批次继续使用私有暂存与原子发布。Native 7.82 Default 7 实窗确认三个 marker 输出 `[0,2)`、`[2,4)`、`[4,5)` 共 3 个文件；延伸 region 至 8 秒不延伸最后 marker 输出。空 marker 名在 `$marker` wildcard 中使用至少两位、左侧补零的 marker number（第 3 个输出 `03`）。fixture、SHA-256、实窗截图和可复现 UI 探针见 [marker Render Bounds evidence](../verification/reaper-parity/fixtures/render-markers-ui-native.tsv) 与 [Render range record](../verification/reaper-parity/project-render-range.md)。

`render_all_markers_freezes_next_marker_ranges_and_output_names` 的定向 nextest 验证 marker 间范围、最后媒体终点、重复/空名称与批次参数；空 marker 名改为两位补零后，最终版本再次由 nightly+Cranelift nextest 1/1 通过。矩阵和 P10 继续 `in_progress`：Custom time range、Selected regions、Selected markers、Render 速率原厂列表、完整命名/焦点和其它 Render 能力仍未完成。

### 2026-10-13 Render Bounds：Custom time range

Render Bounds 新增原厂菜单项 `Custom time range`。首次打开 Render 窗口时，Start/End 从项目 sample 0 和媒体内容终点初始化为项目时钟文本；选中该 bound 后可编辑两端，`Length` 即时显示采样边界间时长。输入复用 REAPER 时钟格式解析，包含负项目时钟值；起止映射经 `Project::sample_at_project_time` 处理项目起始时间偏移并舍入到项目采样边界。End 必须至少比 Start 晚一个样本，时间值非法、反向、量化后为空或超出项目样本时禁用提交并给出提示。提交时将起止样本冻结进离线 render settings，之后编辑字段不会改变已排队任务。项目切换时该输入值的原厂持久化/重置语义和 Render 默认 bound 尚未核验。

Linux REAPER 7.82 / Default 7 原厂实窗输入 `0:00.123` 到 `0:02.456` 后显示长度 `0:02.333`，以 PCM24 stereo 48 kHz 输出 111,984 帧，对应项目样本 `[5,904, 117,888)`。截图、WAV/SHA-256 事实及可复现工程 ReaScript 已归档在[Render 范围记录](../verification/reaper-parity/project-render-range.md)与[fixture](../verification/reaper-parity/fixtures/render-custom-range-native.tsv)。原厂捕获时 Tail checkbox 未勾选；AAADAW 当前仍始终应用独立的 Tail duration，故本切片验证 bounds 输入/边界而不声称未尾音输出帧数完全一致。`Selected regions`、`Selected markers`、完整 Tail toggle/default、Render 速率菜单、Wildcards/Presets/Metadata 与原厂完整窗口验收仍缺。

`custom_render_range_initializes_from_project_clock_and_content_end_once`、`custom_render_range_maps_project_clock_values_to_sample_boundaries_and_freezes_them`、`custom_render_range_rejects_invalid_out_of_project_and_empty_ranges` nightly+Cranelift（测试调试符号关闭）定向 nextest 为 3/3 passed；marker 最终补零断言同一配置 1/1 passed。Stable all-target `cargo clippy -p aaadaw --all-targets -- -D warnings`、fmt、diff 与 parity CSV 92 个唯一 ID/15 列/Render 引用检查通过。Stable 默认调试符号的 `aaadaw` 测试链接运行约 60 分钟仍未进入测试，随后中止且不计作通过；nightly 测试构建保留既有 iced/wgpu `TimelinePipeline: MaybeSync` recursion-depth warning，Stable Clippy 无警告。`RENDER-BOUNDS-001`、`UI-028` 与 P10 保持 `in_progress`。

### 2026-10-13 Render Tail 默认状态与有界渲染清理

REAPER 7.82 Render 将 Tail 显示为未勾选的 checkbox，旁边是禁用的 `1000 ms` 输入。AAADAW 现复刻默认关闭状态，尾音时长改为整数毫秒；关闭时不校验禁用输入且将 0 ms 冻结进离线任务，打开时校验并冻结用户输入。真实 AAADAW Render 窗口截图记录默认和启用 `1250 ms` 两种状态；窗口增高以显示当前完整控件，并在较小窗口中使用滚动容器。[截图 fixture](../verification/reaper-parity/fixtures/render-tail-aaadaw.tsv)。

扩展离线渲染回归验证 44.1 kHz 输出在 1.25 秒尾音和关闭尾音两种情况下的精确帧数。测试同时发现自定义范围早于媒体 EOF 时 feeder 会因队列满而在 join 阶段停住；渲染边界已经消费完毕后，现在会取消未读的 feeder 剩余段并等待线程结束，避免离线任务永久挂起。该清理仅在离线 worker 中执行，不触及实时音频回调。

nightly+Cranelift `aaadaw` Render 定向 nextest 16/16 passed；`aaadaw-app` 的 `project_render_time_selection` 1/1 passed，覆盖有/无 Tail；Stable `cargo clippy -p aaadaw --all-targets -- -D warnings`、fmt 与 diff 检查通过。RENDER 范围行与 P10 继续 `in_progress`：Tail 最大值、键盘焦点、所选范围之后的时间线材料、原厂采样率列表、Selected regions/markers、多声道、Presets/Metadata 和整个对话框的原厂交互仍未完成。

### 2026-10-13 Render Bounds：Selected regions 与 Selected markers

Render Bounds 新增 `Selected regions` 和 `Selected markers`。Region/Marker Manager 支持行选择状态跨窗口关闭/重开保留；Linux Ctrl-click（macOS Command-click）可切换多选，普通点击未选行则替换当前集合。所选区域各自输出半开区间；所选标记沿用 All project markers 的规则，结束于下一个项目标记（无论其是否选中），最后一段延至最后媒体内容终点，零长度段不输出。无选择时按 REAPER 7.82 行为回退到全部可渲染区域/标记。

原厂 REAPER 7.82 / Default 7 实窗确认两种 bounds 均显示 2 个文件，空选择回退及 Manager 多选持久化也已探测。AAADAW 实窗截图确认 Selected markers 的两项计数与跨窗口保存的多选；Selected regions Manager 多选、Native Render 数量及范围计算由截图/单测覆盖。所有截图 SHA-256、原厂输出帧数和证据边界见 [selected-bounds fixture](../verification/reaper-parity/fixtures/render-selected-bounds.tsv)。AAADAW 视觉 fixture 未写出 Selected regions WAV，因此没有将对应音频输出视作实机验收。

Selected-bounds 定向 nightly+Cranelift 测试 1/1，`aaadaw` 全量测试 508/508 passed；Stable all-target Clippy `-D warnings`、fmt 与 diff 检查通过。Cranelift 构建仍发出 iced/wgpu `TimelinePipeline: MaybeSync` recursion-depth future-incompatible warning。`RENDER-BOUNDS-001` 与 P10 继续 `in_progress`：完整 Render 速率菜单、Razor-area Source、Presets/Metadata、完整原厂焦点与禁用态及 Selected regions 的 AAADAW 实窗渲染输出仍待实现/验证。

### 2026-10-13 Render Source：Selected media items

Render Source 新增 `Selected media items`。Arrange 中选择的音频 item 按其 source offset 和长度直接渲染，每个 item 一份 WAV；item fades、输出采样率转换、格式、Mono/Stereo 映射和峰值归一化继续复用离线输出管线。多文件先进入目标目录内的私有 staging，再一次性发布；入队快照中找不到的 item ID 在写文件前拒绝。选中 MIDI 时明确提示当前只支持 audio items。该模式忽略项目 Bounds 与 Tail，Render 窗口显示所选媒体计数并禁用 Tail 控件。项目与实时播放状态不变。

Linux REAPER 7.82 / Default 7 实窗 Source 菜单确认该模式显示 `1 media item` 与 1 个输出；实际渲染为 48 kHz、PCM24、立体声、48,000 帧。常量测试源 L/R PCM24 为 `[1048576, 3145728]`，选中 item 的多个采样点与源逐整数相同。Render/完成窗口截图、SHA-256、样本点、原生探针和对照限制见 [Render Source 验证记录](../verification/reaper-parity/render-source.md)。

新增 audio-only 单 item、失效 item 预检、多 item 独立 trims 和批次取消/发布回归。Stable all-target Clippy `-D warnings` 通过；nightly+Cranelift workspace nextest 全量 999/999（66 个测试二进制）通过。全量测试使用绝对 `CARGO_TARGET_DIR`，避免 CLAP fixture 子进程从 crate 工作目录解析相对路径；Cranelift 仍报告既有 iced/wgpu recursion-depth future-incompatible warning。`RENDER-SOURCE-001`、`RENDER-BOUNDS-001`、`UI-028` 和 P10 继续 `in_progress`：MIDI/take FX、item FX 与原厂音量/自动化行为未建模或未测；当前 `prefix-item_NN.wav` 命名不是完整 wildcard 规则；Region matrix、Razor source、完整原厂命名/Render UI 和 Android 多文件选择仍未完成。

### 2026-10-14 Action List Filter 焦点

提交：`4fb4a03`。

REAPER 7.82 / Default 7 的隔离 Xvfb 会话实测 Action List Filter：输入 `undo` 后按 `Ctrl+Z`，Filter 和工程保持不变；关闭列表后，同一按键在主窗口撤销临时轨道。Filter 内空格不会启动传输，Escape 关闭列表。原厂状态、探针与截图哈希见 [Action List 焦点记录](../verification/reaper-parity/input-focus.md)及其 fixture。

AAADAW 新增 `action_list_filter_focus_contains_global_shortcuts_and_escape_closes_the_window`，通过实际 `RuntimeKeyboardEvent` 窗口分发验证 Filter 窗口隔离全局快捷键、主窗口 Ctrl+Z 仍执行 Undo、Escape 关闭 Action List。该回归单独通过 1/1；`aaadaw` 全量 nightly+Cranelift nextest 在 `DISPLAY=:126` 下 1011/1011 通过（两个 CLAP GUI 测试需要可连接的 X11 显示）。Cranelift 仍报告既有 iced/wgpu `TimelinePipeline: MaybeSync` recursion-depth future-incompatible 警告。`INPUT-FOCUS-001` 继续 `in_progress`：其它文本字段、AltGr/布局、脚本编辑器和其它 Action Section 的焦点穿透仍待逐项验证。

### 2026-10-14 Action List Filter Enter

提交：`12ae6ee`。

REAPER 7.82 / Default 7 原厂探针确认 Filter 输入 `undo` 后第一次 Enter 只选择 `Edit: Undo` 并移开 Filter 焦点，第二次 Enter 才撤销临时轨道。ReaScript 在第一次 Enter 后记录 1 条轨道/工程状态计数 2，第二次后记录 0 条轨道/状态计数 3；播放保持停止。原厂截图、状态与哈希见 [Enter fixture](../verification/reaper-parity/fixtures/action-list-enter-native.tsv)。第二次输入时独立音频硬件错误对话框遮住 Action List，故只保留该步探针状态，没有归档遮挡截图。

AAADAW Filter 现在将第一次 Enter 作为结果提交并移开文本焦点；无可见选择时自动选择首个匹配结果。第二次 Enter 才由 Action List 路由执行选中动作。回归 `action_list_filter_enter_focuses_results_before_running_selected_action` 通过，实窗截图显示第一步轨道仍在、第二步轨道已撤销，证据见 [AAADAW Enter fixture](../verification/reaper-parity/fixtures/action-list-enter-aaadaw.tsv)。

nightly+Cranelift 定向回归 1/1；`aaadaw` 全量 nextest 在无窗口管理器的 `DISPLAY=:124` Xvfb 下 1012/1012 通过。首次用带窗口管理器的显示运行全量时，CLAP X11 fixture 没能在根窗口发布标题；同一用例在无窗口管理器的 Xvfb 单独通过，随后全量也通过。Stable all-target Clippy `-D warnings`、fmt、`git diff --check`，以及 parity CSV 92 个唯一 ID/15 列和新增证据路径检查通过。Cranelift 测试构建保留既有 iced/wgpu `TimelinePipeline: MaybeSync` recursion-depth future-incompatible warning。`INPUT-FOCUS-001` 继续 `in_progress`：其它文本字段、AltGr/键盘布局、脚本编辑器、插件/浮动窗口和其它 Action Section 穿透例外仍待逐项探测。

### 2026-10-14 Action List 结果方向键

提交：`5828129`。

REAPER 7.82 / Default 7 实窗观察：Filter 聚焦时 ArrowDown 不移动结果选择；Enter 把焦点交给结果列表后，ArrowDown/ArrowUp 在可见结果中逐行选择；第一行向上、最后一行向下均停留在当前行，不循环。探针传输保持停止，按键只改列表选择，没有运行动作。各状态、截图与 SHA-256 见 [原厂方向键 fixture](../verification/reaper-parity/fixtures/action-list-arrow-native.tsv)。

AAADAW 的 Action List 键盘路由现在对未被输入控件消费、无修饰键的 ↑/↓ 选择相邻可见结果，并在首尾钳制。`action_list_result_arrows_select_visible_actions_and_clamp_at_bounds` 验证 Filter 捕获按键时不改选择、提交后选择首项、上下移动及两端停留。真实界面查询 `item` 后，第一次 Enter 高亮 `Insert: MIDI item`，ArrowDown 高亮第二项 `Item: Item properties...`；两项都未执行。截图与哈希见 [AAADAW 方向键 fixture](../verification/reaper-parity/fixtures/action-list-arrow-aaadaw.tsv)。

nightly+Cranelift `aaadaw` 全量 nextest 1013/1013 passed、0 skipped；Stable all-target Clippy `-D warnings`、fmt、`git diff --check` 及 parity CSV 92 个唯一 ID/15 列/新增证据路径检查通过。Cranelift 测试构建继续报告既有 iced/wgpu `TimelinePipeline: MaybeSync` recursion-depth future-incompatible warning。`INPUT-FOCUS-001` 继续 `in_progress`：轨道名编辑器与 Action List Filter/结果列表已有首批验证；其它文本字段、AltGr/键盘布局、脚本编辑区、插件/浮动窗口及其它 Action Section 焦点例外未逐项覆盖。


### 2026-10-14 Action List Home/End 导航

提交：`e90228d`。

REAPER 7.82 / Default 7 原厂会话确认：Filter 输入 `undo` 后 Enter 交给结果列表，Home 选择首项、End 选择末项；按键只改选择，没有运行动作。AAADAW 实窗输入 `item` 后 Enter 再按 End，会选择末项并将列表滚到底部；Home 选择首项并滚回顶部。空工程保持零轨，传输停止。原厂与 AAADAW 截图、步骤及 SHA-256 见 [边界键 fixture](../verification/reaper-parity/fixtures/action-list-boundary-native.tsv) 与 [AAADAW fixture](../verification/reaper-parity/fixtures/action-list-boundary-aaadaw.tsv)。

AAADAW 为结果列表增加稳定滚动 ID；Home/End 选择首/末个可见过滤结果，并将相应边界滚入视口。`action_list_result_navigation_selects_visible_actions_and_clamps_at_bounds` 同时覆盖过滤焦点分流、Home/End、方向移动及方向键首尾钳制。nightly+Cranelift `aaadaw` nextest 537/537 passed、0 skipped；Stable `aaadaw --all-targets` Clippy `-D warnings`、fmt、diff 和截图哈希/fixture/矩阵引用检查通过。Cranelift 构建仍报告既有 iced/wgpu `TimelinePipeline: MaybeSync` recursion-depth future-incompatible warning。`INPUT-FOCUS-001` 继续 `in_progress`：其它文本字段、AltGr/键盘布局、脚本编辑器、插件/浮动窗口及其它 Action Section 的穿透例外尚未逐项核验。


### 2026-10-14 Action List 长列表方向滚动

提交：`7e0cf27`。

Linux REAPER 7.82 / Default 7 的 `item` 搜索结果在 Enter 后连续 ArrowDown 18 次，选中行进入列表下方视口并触发滚动，传输仍停止且没有运行动作。AAADAW 的相同输入保持选中项可见；再按 ArrowUp 8 次，列表向前滚动到较早结果。原厂和 AAADAW 截图、步骤及哈希见[长列表方向键 fixtures](../verification/reaper-parity/fixtures/action-list-arrow-scroll-native.tsv)与[AAADAW fixtures](../verification/reaper-parity/fixtures/action-list-arrow-scroll-aaadaw.tsv)。

AAADAW 为结果行固定高度，并按当前窗口尺寸跟踪滚动视口；方向键只在目标行将离开可视区域时滚动，过滤条件改变时回到列表顶部。已有导航回归增加首尾偏移钳制、滚到底部和反向滚动检查。nightly+Cranelift `aaadaw` nextest 537/537 passed、0 skipped；Stable all-target Clippy `-D warnings`、fmt、diff、fixture 哈希和矩阵证据链接检查通过。Cranelift 保留既有 iced/wgpu `TimelinePipeline: MaybeSync` recursion-depth future-incompatible warning。`INPUT-FOCUS-001` 继续 `in_progress`，其它编辑器、键盘布局、插件/浮动窗口和 Action Section 焦点例外仍未逐项验收。


### 2026-10-14 主菜单命令边界循环

提交：`168d9bb`。

REAPER 7.82 / Default 7 原厂 File 菜单打开后，ArrowUp 从初始顶部位置选中末项 Quit，菜单保持打开，动作没有运行。AAADAW 实窗 ArrowUp 选中当前 File 菜单的最后一个启用项 Project settings...；再按 ArrowDown 回到 New project。两端截图、选中项和工程状态见[菜单边界键 fixtures](../verification/reaper-parity/fixtures/menu-keyboard-boundary-native.tsv)与[AAADAW fixtures](../verification/reaper-parity/fixtures/menu-keyboard-boundary-aaadaw.tsv)。该对照只验证键盘循环，不代表菜单项内容相同。

`keyboard_navigation_switches_menus_and_wraps_enabled_commands` 现在直接断言 File 菜单启用项从首项向上循环至末项、再向下循环回首项。nightly+Cranelift 定向 nextest 1/1 passed；截图哈希、矩阵链接、fmt 和 diff 检查通过。`SHELL-MENU-KEYBOARD-001` 仍为 `in_progress`：Help 文档入口、完整 Options、Item/Track 命令集、禁用/勾选态与文本焦点仍有差距。

### 2026-10-14 Arrange 与 Mixer 同窗布局核验

提交：`72c0129`。

核对发现 `SHELL-DOCK-001` 的“当前互斥 workspace”描述过时：Linux Desktop profile 已默认在同一窗口显示 Arrange 和 Mixer；Arrange/Mixer 工作区切换仅用于 Touch profile。用全新 REAPER 7.82 配置、Default 主题和 Dummy Audio（虚拟环境无硬件）复验冷启动默认布局，再插入一条空轨确认同一 Track 同时出现在 TCP/MCP；AAADAW 实窗也显示 Arrange、Mixer 和同一 Audio 1。原厂默认、原厂同轨和 AAADAW 截图及哈希见 [Arrange/Mixer fixture](../verification/reaper-parity/fixtures/arrange-mixer-dock.tsv)。

nightly+Cranelift 的两项 focused nextest 2/2 passed、535 skipped，覆盖 Mixer/Media Browser 开关、独立比例、轨道 ID 保留和布局保存/恢复；编译保留既有 iced/wgpu `TimelinePipeline: MaybeSync` 递归深度警告。矩阵修正为 `in_progress`，不以截图或这两项回归宣称 Docker parity 完成：边缘停靠、标签页、浮动窗口、默认几何和 TCP/MCP 控件双向编辑仍待验证。

### 2026-10-14 Item 菜单空工程 Select all

提交：`41678b3`。

Linux REAPER 7.82 / Default 7 在一条空轨且无媒体项时仍启用 Item > Select all。AAADAW 已将现有选择动作接入 Item 菜单并使用原厂命令名称；空工程执行保持无选择、无 revision 变化。原厂和 AAADAW 实窗截图、环境、空工程状态及 SHA-256 见 [Item Select all fixture](../verification/reaper-parity/fixtures/menu-item-select-all.tsv)。两图显示菜单条目和启用状态，不构成完整布局对照：AAADAW 当前把它放在首项，REAPER 前面还有尚未实现的命令。

`item_menu_exposes_enabled_select_all_for_an_empty_project` 和原有的音频/MIDI 全选测试通过；更新旧测试对空工程禁用的断言。nightly + Cranelift `aaadaw` 全量 nextest 538/538 passed、0 skipped；Stable all-target Clippy `-D warnings`、fmt、diff、parity CSV 92 个唯一 ID/15 列及新增截图哈希检查通过。Cranelift 仍报告既有 iced/wgpu `TimelinePipeline: MaybeSync` recursion-depth future-incompatible warning。`SHELL-MENU-KEYBOARD-001` 保持 `in_progress`：其它 Item/Track 命令与子菜单、完整禁用/勾选状态、命令顺序、视觉及焦点行为仍未完成。

### 2026-10-14 Item 菜单 Remove items

提交：`c031790`。

REAPER 7.82 / Default 7 原厂空工程菜单将 `Remove items` 放在首项并在没有选择时禁用；AAADAW 现将已有删除动作按该名称和菜单优先级显示，空选择时禁用，`Select all` 放在其后的分隔线之后。选择 MIDI 项后 `Remove items` 可用，删除是一项可撤销工程编辑；Action List 仍接受 “Delete selected items” 搜索名。原厂空工程截图复用原有 Item 菜单捕获，AAADAW 当前实窗截图及状态/哈希见 [Remove items fixture](../verification/reaper-parity/fixtures/menu-item-remove.tsv)。所附截图只比较无选择状态；选中后可用、删除和撤销由单元回归验证。

`item_menu_exposes_native_remove_and_select_all_availability_for_an_empty_project` 验证空选择菜单首项和 Select all 状态；`item_menu_remove_items_deletes_selected_media_and_is_undoable` 验证 MIDI 删除/撤销。nightly + Cranelift `aaadaw` 全量 nextest 539/539 passed、0 skipped；Stable all-target Clippy `-D warnings`、fmt、diff、parity CSV 92 个唯一 ID/15 列和 fixture 哈希/路径检查通过。Cranelift 仍报告既有 iced/wgpu `TimelinePipeline: MaybeSync` recursion-depth future-incompatible warning。`SHELL-MENU-KEYBOARD-001` 继续 `in_progress`：原厂在 Select all 前的 Trim/Copy/Enclose 等命令、其它 Item/Track 命令和子菜单、禁用/勾选态及完整焦点与视觉对照仍待实现和验收。

### 2026-10-14 Item 菜单 Trim items to selected area

实现提交：`0d328ee`。

将 Item > `Trim items to selected area` 注册为菜单与 Action List 命令，放在 `Remove items` 之后、`Select all` 分隔线之前；空工程时启用。选中的音频与 MIDI 项分别按样本/时钟交集裁切，严格落在选区外的项保持不变，恰好贴住选区端点的项删除。左裁会递增音频源样本偏移或 MIDI 内容偏移；多项修改合并成一个可撤销事务，并保持仍存在项的选择状态。无时间选区、零长度选区或无所选项时不修改工程；播放/录制等工程编辑忙碌状态会阻止执行。

Linux REAPER 7.82 / Default 7 的 ReaScript 40508 探针记录内部裁切、左右交叠、严格外侧保留、两端贴边删除及无选区 no-op。原厂裁切画面、原厂空工程菜单、AAADAW 空轨 Item 菜单截图、状态和 SHA-256 见 [Trim fixture](../verification/reaper-parity/fixtures/menu-item-trim.tsv)。实窗截图验证菜单名称、已实现命令顺序和空工程启用状态。原厂操作探针使用 MIDI 项；AAADAW 的音频源偏移与混合媒体事务目前由单元测试验证，尚未用 REAPER 音频源做原生逐项对比，因此不把这部分记录为已完成的原厂验收。

Stable `cargo nextest run -p aaadaw` 542/542 passed；Stable all-target Clippy `-D warnings`、fmt 和 `git diff --check` 通过。本次本地构建关闭调试符号，避免大型测试链接超出工作区资源；这只影响忽略的 `target/` 产物。`SHELL-MENU-KEYBOARD-001` 继续 `in_progress`：原厂在 Trim 与 Select all 之间还有 Copy loop of selected area 和 Enclose media items in razor edit 等命令；其它 Item/Track 命令、完整视觉和焦点行为仍待实现与验收。

### 2026-10-14 Item 菜单 Copy loop of selected area

实现提交：`f33aaa8`。

新增 Item > `Copy loop of selected area of items`（Ctrl+Alt+L）和 Edit > `Paste`（Ctrl+V）。复制只记录所选音频项里完整覆盖时间选区的源片段，不修改工程状态和原选择；粘贴从编辑光标插入，保留源媒体位置、可用目标轨道间的相对顺序，并作为一步撤销。空工程中的 Copy 仍启用并安全 no-op；剪贴板为空时 Paste 禁用。

Linux REAPER 7.82 / Default 7 原厂探针确认：选区 3–7 秒、源项 0–10 秒时，Copy 不改变项属性或工程 state-change count；在 12 秒粘贴生成 4 秒、`B_LOOPSRC=1` 的 WAV source section。部分覆盖选区的所选项会被排除；两项都完整覆盖时，粘贴保留各自轨道位置。AAADAW Item/Edit 菜单实窗截图、原厂探针结果、环境和 SHA-256 见 [Copy loop fixture](../verification/reaper-parity/fixtures/menu-item-copy-loop.tsv)。截图只验证菜单文字、快捷键和空剪贴板启用状态，不构成完整窗口复刻验收。

Stable `cargo nextest run -p aaadaw` 543/543 passed；Stable all-target Clippy `-D warnings`、fmt、diff 检查通过。原厂 REAPER 7.82/Linux ReaScript 探针通过；AAADAW 空轨菜单截图和 CSV 93 个唯一 ID/15 列检查通过。`ITEM-AUDIO-COPYLOOP-001` 与 `SHELL-MENU-KEYBOARD-001` 继续 `in_progress`：AAADAW 尚未持久化或渲染 `B_LOOPSRC`，因此粘贴项延长后的循环行为仍不完整；目前只支持同一工程的音频源和已有目标轨道，不复制其它 item/take mix、淡化或播放率数据，跨工程媒体剪贴板与通用 Paste 来源也未完成。Item 菜单其余命令和完整界面/焦点对照仍待实现与验收。

### 2026-10-14 Item 菜单 Enclose media items in razor edit

实现提交：`1f36222`。

新增 Item > `Enclose media items in razor edit` 与 Action List `Razor edit: Enclose media items, including space between items`。所选音频和 MIDI 素材共同决定全局起止边界；每条含选中素材的轨道都获得一段包含素材间空隙的 media razor area，其它轨道的旧区域被清除；无选择时清除全部区域。动作会取消素材选择，区域修改作为一步撤销。样本时钟区域已加入工程快照及 SQLite schema 41，并绘制为轨道内半透明时间线覆盖。

Linux REAPER 7.82 / Default 7 的 action 42409 探针确认四个音频/MIDI 选择项在两条轨道创建 2–12 秒区域、清空选择且只产生一个状态变更；再次执行会替换旧区域，无选择执行清空全部区域，Undo/Redo 清除和恢复区域。原厂与 AAADAW 同为 1540×960 的三轨时间线截图使用相同的 Audio/MIDI 时间位置和 2–12 秒区域，显示两轨覆盖、间隙及第三轨无区域；原厂窗口在虚拟显示中没有打开音频设备，传输保持停止。菜单截图、结果 TSV、环境和哈希见 [Enclose razor fixture](../verification/reaper-parity/fixtures/menu-item-enclose-razor.tsv)。

Stable `cargo nextest run -p aaadaw-core -p aaadaw-storage -p aaadaw` 730/730 passed、0 skipped；Stable 跨三包 all-target Clippy `-D warnings`、`cargo fmt --check`、`git diff --check` 通过。Parity matrix 有 94 个唯一 ID、15 列，fixture 路径和截图/结果哈希均已验证。`ITEM-RAZOR-ENCLOSE-001` 与 `SHELL-MENU-KEYBOARD-001` 保持 `in_progress`：AAADAW 暂未实现 envelope razor area 及其它 Razor edit 命令；Undo 目前不恢复执行前的 item 选择，而原厂脚本探针的选择撤销结果受构造历史影响，仍待明确核验。其余 Item/Track 菜单和完整界面一致性继续推进。

### 2026-10-14 Item 菜单 Split items at cursor

实现 Main action 40012 的完整编辑路径。Linux REAPER 7.82 / Default 7 原厂探针确认 S 快捷键的 action 名称 `Item: Split items at edit or play cursor (select right)`：有选择时只切选中项并只选择右段；无选择时切割所有跨越光标的项目并只选择右段；光标正好位于边界上时不改工程。AAADAW 的 Action List 名称现已与原厂一致，Item 菜单使用短标签 `Split items at cursor` 和 S；有选择和无选择两种路径都按原厂规则更新选择，音频源采样率与源偏移被保留，MIDI 按 tick 切分。

原厂输出 TSV 和 REAPER/AAADAW 1540×1160 实窗截图、哈希与比较边界记录在 [Split cursor fixture](../verification/reaper-parity/fixtures/menu-item-split-cursor.tsv)。AAADAW 实窗通过实际 S 键切开双轨 MIDI 工程并选择右段；屏幕像素定位落在 5.007 秒，原厂探针在 5.000 秒。回归覆盖选中 MIDI 项、无选择多轨、音频 44.1 kHz 源在 48 kHz 工程中的偏移换算、边界 no-op、Action List 名称和单步 Undo/Redo。

Nightly + Cranelift workspace nextest 1027/1027 通过，隔离 Xvfb `DISPLAY=:223` 下的 UI/CLAP 测试均运行；Stable workspace all-target Clippy `-D warnings`、fmt、diff 检查通过。Parity matrix 有 96 个唯一 ID、15 列，cursor-split TSV 与截图哈希已校验。`ITEM-SPLIT-CURSOR-001` 仍为 `in_progress`：尚未制作音频项的 REAPER 原厂实窗对照，也没有撤销 item 选择恢复；Item 菜单其它命令、更多光标/播放状态交互和完整 UI 对齐继续推进。

### 2026-10-14 Item 菜单 Split items at time selection or razor edit

实现提交：`e30c3ee`。

新增 Main action 40061 对应的 Action List 命令 `Split items at time selection or razor edit`，Item 菜单显示原厂短标签 `Split items at time selection` 和 Shift+S。时间选区与选中项并存时仅分割选中项，并只选择时间选区内部片段；时间选区有效但没有项选择时分割全工程素材并选中内部片段；没有时间选区时按每条轨道的 media razor area 边界分割该轨道所有音频/MIDI 项、清除 area 并选中 area 内片段。音频源偏移按源采样率换算，MIDI 继续按 tick 分割；每种路径都在单个工程事务中完成。

Linux REAPER 7.82 / Default 7 原厂 ReaScript 探针覆盖上述三种模式，确认区域边界、未选中素材保留、选区内部片段选择和单次 state change。原厂与 AAADAW 都实际运行 1540×1160 窗口并截取时间线及 Item 菜单：四轨 0–10 秒素材中，首轨 3–7 秒片段被选中，其余轨道保持完整；两个菜单截图都显示 Shift+S，区域被消耗且无时间选区后命令处于禁用状态。原厂 TSV、菜单/时间线截图及 SHA-256、AAADAW 实窗截图与对比边界见 [Split fixture](../verification/reaper-parity/fixtures/menu-item-split-time-or-razor.tsv)。

nightly + Cranelift workspace `cargo xtest` 1025/1025 通过（CLAP GUI 测试在隔离 Xvfb `DISPLAY=:223` 下运行）；Stable workspace all-target Clippy `-D warnings`、fmt、diff 检查通过。Parity matrix 有 95 个唯一 ID、15 列，五个原厂结果/截图与 AAADAW 截图哈希均已校验。`ITEM-SPLIT-TIME-RAZOR-001` 和菜单/整体 UI 仍保持 `in_progress`：时间选区与 Razor 同时存在时 REAPER 的优先级尚未探测；Undo/Redo 目前只恢复工程素材，不恢复之前/之后的 item 选择；envelope Razor、媒体分组语义及完整 Item 菜单仍待实现。

### 2026-10-14 Item 菜单 Heal splits in items

新增 Main action 40548 `Heal splits in items`，无默认快捷键；无媒体项选择时 Item 菜单禁用，选择项后启用。Linux REAPER 7.82 / Default 7 原厂探针确认：动作不愈合 MIDI；音频 split 的两段都选中且时间/源位置连续时合为一个已选音频项，一次工程状态变更；只选一侧、无选择、时间有 gap 或源偏移不连续均 no-op；连续三段一次合并。原厂探针在本轮通过 `DISPLAY=:223` 重跑，结果文件与脚本一致。

AAADAW 现将菜单/Action List 命令接入同一事务；只合并同轨、相邻、共同选中且同媒体引用的音频项，并按工程采样率和源采样率检查源帧连续性。左侧 item ID 和其余已选 MIDI 项保留，后续片段删除；两段/三段合并均一步 Undo/Redo。回归还检查不同媒体引用、gap、部分选择、MIDI 和源断续 no-op，以及 44.1 kHz 源在 48 kHz 工程的换算。原厂时间线截图从探针保存的 after-project 重新打开，确认最终只有一项；原厂/AAADAW 实窗菜单与时间线截图、探针 TSV 和 SHA-256 见 [Heal splits fixture](../verification/reaper-parity/fixtures/menu-item-heal-splits.tsv)。截图只覆盖菜单标签、无快捷键和合并结果，不代表整套 Item 菜单或窗口已对齐。

Nightly + Cranelift workspace nextest 1028/1028 passed（0 skipped），包括定向 heal 回归；Parity matrix 现有 97 个唯一 ID、15 列，截图和原厂结果文件 SHA-256 已校验。`ITEM-HEAL-SPLITS-001` 仍为 `in_progress`：原厂不同媒体引用语义、自定义 fades/mix、group/frozen item 行为尚未覆盖，AAADAW 也未实现完整混音/淡化重建；Item 菜单及整体 UI 继续推进。

### 2026-10-14 Item 菜单 Nudge/set items...

Linux REAPER 7.82 / Default 7 的 Item 菜单在 `Select all` 后显示 `Nudge/set items...` 和 `N`；Main action 41226 的 Action List 名称是 `Item edit: Nudge/Set...`。AAADAW 将菜单和 Action List 收敛为同一条命令，实窗验证菜单入口、N 快捷键和 528×121 对话框。原厂探针确认默认 Position Nudge 将所选音频或 MIDI 项右移 1 ms，保留长度、源偏移和选择；无选择时不改变素材。原厂、AAADAW 对话框和 target 下拉截图、原厂结果及哈希见 [Nudge/Set 验收记录](../verification/reaper-parity/item-nudge.md)。

AAADAW 实现 Position 与 Edit cursor 的 Nudge/Set 路径、混合媒体共享位移、绝对/相对 Set 和单步 Undo/Redo；实窗中对选中 MIDI 项点击 `Nudge right` 后，摘要和时间线共同反映 0.251 秒的位置；其余六种 item target 明确失败关闭。Nightly + Cranelift workspace nextest 1037/1037 passed、0 skipped；Stable workspace all-target Clippy `-D warnings`、fmt、`git diff --check`、Parity CSV 15 列/98 个唯一 ID及新 fixture 的 SHA-256 检查通过。构建仍报告既有 iced/wgpu `TimelinePipeline: MaybeSync` recursion-depth future-incompatible warning。

`ITEM-NUDGE-SET-001` 继续 `in_progress`：已按 REAPER 原厂探针实现 Audio 的 left trim、left edge、end position、right trim，以及 Audio/MIDI duplicate；Contents、这些目标的 MIDI 响应、Set/Snap 组合与原厂 Undo 语义仍待验证。Item 菜单和整套界面仍远未达到 100% 对齐，后续继续补齐这些边界与其余菜单/窗口交互。

### 2026-10-14 REAPER Nudge target coverage

实现提交：`e745e09`。

以 REAPER 7.82 / Default 7 的实际 target 探针为依据，加入 Audio Left Trim、Left Edge、End Position、Right Trim 的 Nudge 语义，以及 Audio/MIDI Duplicate；副本单步 Undo/Redo 并成为当前选择。Left Trim 根据素材源采样率换算源偏移，组合操作在一个事务内校验。原厂八种 target 的实测状态见 [native target TSV](../verification/reaper-parity/fixtures/item-nudge-targets-native.tsv)，哈希记录在同目录 `item-nudge.tsv`。

Nightly + Cranelift Nudge focused nextest 11/11 passed；Stable `cargo clippy -p aaadaw --all-targets -- -D warnings`、fmt、diff 检查通过。Parity matrix 保持 98 个唯一 ID / 15 列，native target TSV 哈希与矩阵路径校验通过。本轮环境没有 Xvfb/虚拟显示服务，因此没有为新增 target 操作制作新的 AAADAW 实窗截图；此前的对话框与 Position Nudge 实窗截图仍在 Nudge 验收记录中。Cranelift 构建仍报告既有 iced/wgpu `TimelinePipeline: MaybeSync` future-incompatible warning。

`ITEM-NUDGE-SET-001` 保持 `in_progress`：Contents、各 target 对 MIDI 的原厂响应、目标专属 Set/Snap、Undo 历史及新 target 的 AAADAW 截图仍待补齐；Item 菜单和完整界面对齐继续推进。

### 2026-10-14 Item Nudge Contents 边界内移动

实现提交：`1f7aaa9`。

Contents / Nudge 现在只在媒体元数据可用且移动后的整个 source section 仍位于文件帧范围内执行；按源采样率反向改源偏移，保留 item 的 timeline 位置和长度，跨越源起点/终点或混选 MIDI 时整批拒绝。原厂 REAPER target 探针在 250 ms 下观测到 source offset 5.00 s 移至 4.75 s；其循环源绕回仍不能由 AAADAW 当前模型表示，不把边界行为标为完整等价。

Nightly + Cranelift Nudge focused nextest 11/11 passed，覆盖 44.1 kHz source 与 48 kHz project 的双向换算、两个 source 边界的安全拒绝及 Undo；Stable `cargo clippy -p aaadaw --all-targets -- -D warnings`、fmt、diff 检查通过。Parity matrix 保持 98 个唯一 ID / 15 列，原厂 target fixture 哈希与矩阵引用校验通过。当前容器没有虚拟显示服务，本轮未新增 AAADAW 实窗截图。

`ITEM-NUDGE-SET-001` 仍为 `in_progress`：REAPER 的循环源属性及 Contents 绕回、MIDI 对各 target 的响应、target-specific Set/Snap、Undo 历史和新增操作的 AAADAW 实窗验证继续待办。

### 2026-10-14 Item Copy loop source metadata

AudioItem 现在持久化 `B_LOOPSRC` 与可选 source-section 长度；schema 42 对旧工程以循环关闭/完整源默认值迁移。Copy loop 根据素材采样率保存所选源段长度，Paste 创建带循环源标记的音频项，撤销/重做和 SQLite 保存/重开保留该元数据。正常导入与冻结渲染继续使用循环关闭默认值。

Nightly + Cranelift `aaadaw-core`、`aaadaw-storage` 与 `aaadaw` nextest 749/749 passed；覆盖 schema 41 的增量迁移、旧音频表被重新创建、source-section 往返与 44.1/48 kHz Copy/Paste 换算。Stable all-target Clippy、fmt、diff 检查通过。该步骤只建立数据模型与粘贴状态；实时播放、离线渲染和 Nudge Contents 仍未消费循环标记，因此循环源功能保持 `in_progress`。

### 2026-10-14 B_LOOPSRC source-section playback and Contents wrap

在 schema 42 的循环标志/长度后新增 schema 43 `source_section_start_samples`。旧 schema 42 bounded loop 项以其原有 `source_offset_samples` 回填循环段起点；循环段起点、长度与当前源偏移在 Core/Snapshot/SQLite 中共同校验和往返。Copy/Paste 继续从复制时的源偏移建立新段起点。

后台媒体 worker 现在按 REAPER item 的循环源顺序处理：bounded section 首轮从当前源偏移播放到固定段尾，随后从固定段起点循环；没有 bounded section 的 B_LOOPSRC 项在完整媒体 EOF 后从源头重启。短 bounded section 在解码 worker 内缓存完整循环段，较长循环段和完整源循环由可重启解码器继续读取；这些路径均不在实时 callback 中做文件 I/O 或分配。Mono/stereo、realtime Sinc64、offline Sinc192、媒体 seek、共享 embedded source cache 与超过 1 MiB 的解码路径有回归覆盖。两轨共享嵌入素材的端到端测试验证了实时混音与离线 WAV 中的重复帧。

Audio Contents Nudge 现在按源采样率把偏移限制在 loop source 的固定段内（完整源循环使用媒体帧范围），并保持循环段边界、item 时间位置和长度不变；Undo 仍作为同一步工程修改。复制范围跨过已有 loop 边界时当前明确拒绝，因为单一连续媒体段不能准确表示拼接后的循环内容。

Nightly + Cranelift 的 Core/Storage/Media/App/desktop nextest 为 869/869 passed、0 skipped；Stable workspace all-target Clippy `-D warnings`、fmt 与 diff 检查通过。Linux REAPER 7.82 的 Copy loop 探针曾确认 `B_LOOPSRC=1` 和源段长度；本轮没有重新探测 Contents 回绕边界，也没有为新增音频行为制作 REAPER/AAADAW 实窗截图。`ITEM-AUDIO-COPYLOOP-001` 与 `ITEM-NUDGE-SET-001` 继续 `in_progress`：原厂回绕与跨边界复制语义、Item/take mix/fade/rate 元数据、目标专属 Set/Snap、完整剪贴板与整套 UI 对齐仍待完成。全源循环在当前模型与媒体 worker 可用，但 UI 尚未提供对应的 B_LOOPSRC 切换入口。

### 2026-10-14 Item Properties Loop source

依据 Linux REAPER 7.82 / Default 7 的现有 `Media Item Properties` 原厂截图及 `B_LOOPSRC=1` 探针，普通新插入音频项现默认启用 Loop source；Item Properties 增加 `Loop source` 复选框。切换使用项目历史的一步 undo/redo，并在音频输出打开时拒绝会导致 worker 状态过期的切换。

bounded source section 的起点和长度现在独立于循环开关保存：关掉循环后 worker 从当前源偏移播放到 section 尾部，然后结束，不会重启 decoder 或重复；SQLite 与 snapshot 接受该有效状态，Contents Nudge 仅对实际循环的区段做绕回。旧 schema 的迁移默认值保持关闭，避免改变已有工程。

回归覆盖 Core action 默认值和开关 Undo/Redo、bounded metadata 的 snapshot/storage 往返、关闭后单次媒体播放、属性窗 Apply/Undo/Redo。Nightly + Cranelift 五包 workspace nextest 872/872 passed、0 skipped；Stable workspace all-target Clippy `-D warnings`、fmt、diff 检查及 parity matrix（98 个唯一 ID、15 列）通过。本轮使用的是已有 REAPER 原厂截图作为控件/默认值参照，没有新录制 AAADAW 实窗截图或重跑原厂探针；属性窗口的其余字段、尺寸/布局和像素对齐仍不完整，`ITEM-PROPERTIES-001`、`ITEM-AUDIO-COPYLOOP-001` 继续 `in_progress`。

### 2026-10-14 Media Item mute

参照 catalog 中 Main action 40175 为 `Item properties: Toggle mute`；现有 REAPER Media Item Properties 截图显示 `Mute` 默认未勾选。AAADAW 为 Audio Item 增加可持久化 mute 状态、schema 44 迁移、属性窗草稿/Apply、Undo/Redo 与 Action List command。实时播放控制以一个原子值共同发布 item gain/mute，渲染 callback 在 mute 时将增益置零；实时与 offline PCM 均验证为静音。

Nightly + Cranelift workspace nextest 1057/1057 passed，排除了两项需要 X11 的 CLAP 浮动插件窗口测试（当前环境 `X11 connect failed`）；Stable workspace all-target Clippy `-D warnings`、fmt 与 diff 检查通过。使用的是已有 REAPER 参考图，本轮未新增 AAADAW 截图，也未重新运行原厂静音行为探针。

`ITEM-MUTE-001` 与 `ITEM-PROPERTIES-001` 保持 `in_progress`：当前仅 Audio mute；MIDI/mixed-selection、复制粘贴 mute 状态、原厂菜单与启用边界、属性窗完整布局及 AAADAW 截图对照尚未完成。

### 2026-10-14 MIDI Item mute extension

Main action 40175 现在支持 Audio、MIDI 与混合选择的一步 Undo/Redo。MIDI item mute 进入核心模型、Snapshot 与 schema45；schema44 工程迁移为未静音。重复 MIDI item 保留 mute 状态。实时/离线 MIDI 事件计划都过滤静音 item；播放中含 MIDI 的 mute 命令禁用，并把播放图标脏，在下次启动前按新状态重建。Audio mute 继续走同一个 realtime volume/mute atomic。

Nightly + Cranelift workspace nextest 1063/1063 passed，排除了两项依赖 X11 的 CLAP editor 测试；Stable workspace all-target Clippy `-D warnings`、fmt、diff 检查通过。新增 Core Undo/snapshot/duplicate、SQLite 往返/schema44 迁移、实时/离线 MIDI plan 和混选 Action 回归。仍使用既有 REAPER 参考图，本轮没有 AAADAW 截图或原厂新探针。

`ITEM-MUTE-001` 保持 `in_progress`：MIDI Item Properties 窗口尚未实现，含 MIDI 选择在播放中禁用，Audio loop copy/paste 不保留 mute；native 混选及播放语义、菜单/启用状态、完整窗口布局和截图对照待补。

### 2026-10-14 Audio loop Copy/Paste mute state

Audio loop clipboard entries now retain each source item’s mute bit; `InsertLoopedAudioItem` applies that bit when creating the pasted item, within the same undoable transaction. The multi-track copy test checks muted/unmuted entries, pasted state, Undo removal and Redo restoration. Nightly + Cranelift focused copy test passed; the complete workspace suite (1063/1063, with two X11-dependent tests excluded) and Stable strict Clippy had passed before this final assertion-only test extension. Native REAPER copy-mute behavior and AAADAW screenshots remain unverified; `ITEM-AUDIO-COPYLOOP-001` and `ITEM-MUTE-001` stay `in_progress`.

### 2026-10-14 MIDI Item Properties mute control

F2 / `Item properties: Toggle mute` now opens the properties window for a selected MIDI item. Its Mute checkbox uses a draft; Apply writes one undoable project action, Cancel closes without applying pending edits, and Undo/Redo changes mute without altering MIDI notes. The checkbox message is accepted through the secondary-window event route. This is a mute-only MIDI properties view; the remaining source, timing, take, and fade fields and REAPER window geometry are still absent.

Nightly + Cranelift workspace nextest passed 1064/1064 with two X11-dependent CLAP floating-editor tests excluded; the focused MIDI and Audio item-properties/F2 tests passed 3/3. Stable workspace all-target Clippy `-D warnings`, fmt, and diff checks passed. No AAADAW screenshot or new native REAPER probe was made. `ITEM-MUTE-001` and `ITEM-PROPERTIES-001` remain `in_progress`.

### 2026-10-14 MIDI Item Properties Position and Length

在前一段 MIDI mute-only 属性窗基础上，增加 Position、Length 和 Time/Beats/H:M:S:F/Samples 单位选择。解析使用当前工程起始时间、帧率、速度图和拍号；编辑通过 `EditMidiItem` 保留 MIDI 内容及 source offset。Position/Length 与 Mute 同时修改时汇总为单一 BatchTransaction；非法、零长、溢出输入不创建项目动作，Cancel 丢弃草稿。

回归对四种单位使用负工程起始时间偏移做格式化/解析往返，拒绝零长度和超过 `f64` 精确整数范围的位置；App 测试验证 Position、Length、Mute 一次 Apply/Undo/Redo，并检查音符保持不变。Nightly + Cranelift workspace nextest 1065/1065 passed，排除两项依赖 X11 的 CLAP floating-editor 测试；Stable workspace all-target Clippy `-D warnings`、fmt、diff 检查和 parity matrix 校验通过。本轮没有 AAADAW 截图或 REAPER 新探针。

MIDI Item Properties 仍缺 Start in source、Item timebase、take name/playback rate/pitch/mix、source controls、fades 和完整 526×693 原厂窗口几何；`ITEM-PROPERTIES-001` 与 `ITEM-MUTE-001` 继续 `in_progress`。

### 2026-10-14 MIDI Item Properties Start in source

Start in source 现在在 MIDI 属性窗口可编辑：Time/Beats/H:M:S:F 使用 Time 输入，Samples 使用样本输入。新增 `EditMidiItemPlacement` 核心动作，将 start tick、length ticks 和 source offset ticks 一次校验并写入单一 MIDI item 变更事件，避免组合 Apply 中间态溢出；事件保留音符、CC 和 pitch bend 数据。MIDI placement 改动也会使播放计划失效，在下一次播放前重建。

Nightly + Cranelift workspace nextest 1066/1066 passed，排除两项需 X11 的 CLAP 浮动编辑器测试；Stable workspace all-target Clippy `-D warnings`、fmt、diff 和矩阵结构检查通过。专项 Core Undo/Redo、SQLite 往返与 MIDI 属性 UI 测试 3/3 passed；四种单位及负工程起点、非法数值测试通过。本轮无 AAADAW 截图及 REAPER 新探针。

完整 MIDI 属性仍待 Item timebase、take/source controls、fade、批量/混合值以及原厂尺寸和控件布局；`ITEM-PROPERTIES-001` 与 `ITEM-MUTE-001` 保持 `in_progress`。

### 2026-10-14 MIDI Item Properties Name

MIDI Item Properties 增加 Name 草稿字段。Name、Position、Length、Start in source、Mute 共用一个 Apply transaction；名称沿用 `SetMidiItemName` 的非空及 128 字符限制，任何字段验证失败时整组修改保持未应用。Cancel 会丢弃未应用名称。

App 测试覆盖全部五字段一次 Apply/Undo/Redo、空名称导致批量字段原子拒绝，以及改名后 Cancel。Nightly + Cranelift workspace nextest 1066/1066 passed，排除两项 X11 依赖测试；Stable workspace all-target Clippy `-D warnings`、fmt 和 diff 检查通过。本轮未录 AAADAW 截图或重新探测 REAPER。

MIDI Properties 仍缺 item timebase、take playback rate/pitch/mix、剩余 source controls、fades、批量/混合值和完整窗口几何；不关闭 `ITEM-PROPERTIES-001` 或 `ITEM-MUTE-001`。

### 2026-10-14 Delete by focus：轨道确认与 no-prompting

REAPER 7.82 原生探针在 TCP 点击后同时选中轨道与 MIDI item，再按 Delete，显示 `Delete 1 selected track?`；取消没有项目变化。将焦点保持在 TCP 并运行 Action List 的 `- no prompting` 变体会直接删除轨道及其 MIDI item，原厂状态变化计数只增加一次。AAADAW 现维护 Arrange、TrackPanel、Automation 三类最近目标区；普通 Delete 在 TrackPanel 弹确认框，Escape/Cancel/遮罩取消无修改，确认经现有依赖顺序删除事务执行；no-prompting 使用同一事务但不弹框。Arrange item 和 Automation point 根据各自目标区删除。实窗点击轨道名后 Delete 也已验证进入轨道确认；轨道名现在单击选中、双击才进入名称编辑。焦点状态目前限定为主 Arrange/Mixer 内最后一次目标区交互，不等同全部 REAPER 子窗口和控件的真实键盘焦点。

实窗截图验证了 AAADAW 确认框和无提示删除后的空轨道状态；原厂对应截图及计数证据见 [Delete by focus runtime evidence](../verification/reaper-parity/fixtures/delete-by-focus-runtime-native.tsv)。AAADAW nightly + Cranelift 应用 551/551 测试通过；Stable workspace all-target Clippy `-D warnings` 和 fmt/diff 检查通过。未做完整焦点窗口或像素验收，`INPUT-BIND-001` 仍为 `in_progress`。


### 2026-10-14 Audio Item Invert phase

参考 Media Item Properties 截图中的 `Invert phase`，音频 item 现在有可保存、可撤销的 Take 极性状态。SQLite schema 46 对旧工程迁移为正常相位；新建与冻结 item 默认正常，复制保留原设置。属性窗勾选通过项目 action 应用；实时图将 volume/mute/phase 合并在现有原子状态中，回调逐帧对左右通道乘以 `-1`，不锁定、不分配、不重建图。新建播放/离线 render 图从项目状态初始化。

Core 快照/Undo、Storage schema 38 迁移、App Apply/Cancel/Undo/Redo 和 Engine 实时 PCM 正反相定向测试 4/4 通过；另外验证了离线 WAV PCM 极性，以及 loop-area 复制粘贴在 Undo/Redo 和再次粘贴中保留相位。Nightly + Cranelift workspace nextest 1069/1069 passed，排除两项需 X11 的 CLAP GUI 测试；Stable workspace all-target Clippy `-D warnings`、fmt 与 diff 检查通过。参考图哈希及本轮未录新截图/未探测 REAPER 的限制见 [Item phase verification](../verification/reaper-parity/item-phase.md)。`ITEM-PROPERTIES-001` 仍保持 `in_progress`，属性窗其余 Take/source 字段及完整原厂几何/截图对照未完成。

### 2026-10-14 Multi-item Properties mute

多个 Audio/MIDI item 同时选择时，Item Properties 显示共同 Mute 值；值不一致时明确显示 Mixed values，并提供直接的 Mute all/Unmute all 目标。用户应用后，所有实际需要变化的 item 共享一个 Undo/Redo 步骤。包含 MIDI 的 selection 只有在 MIDI item 确实需要改变 mute 时才会在播放中拒绝修改，避免现有 MIDI 播放计划过期。

回归验证混合 Audio/MIDI selection 的 Mixed 状态、统一 Apply、单步 Undo/Redo、Cancel，以及播放中 MIDI 改变保护和项目 I/O 忙碌时仍可编辑草稿；Nightly + Cranelift aaadaw 应用层 nextest 549/549 passed，0 skipped。Stable workspace all-target Clippy `-D warnings`、fmt 与 diff 检查通过。本轮没有 AAADAW 实窗截图或新 REAPER 探针；批量属性目前只覆盖 Mute，其他字段、混合值布局与完整原厂几何仍未完成，`ITEM-PROPERTIES-001` 保持 `in_progress`。

### 2026-10-14 TCP 轨道名单击与双击编辑

原有交互把轨道名单击直接交给文本输入框，导致 Delete 被输入控件消费，无法像 REAPER TCP 那样以轨道为目标。名称现在默认是只读标签：单击选择轨道并设置 TrackPanel 删除目标，双击进入现有可撤销名称编辑器；编辑中的文本控件继续接收键盘输入。实窗验证了单击后 Delete 打开轨道确认，以及双击后名称输入框获得焦点；截图和哈希分别记录在 [Delete by focus runtime evidence](../verification/reaper-parity/fixtures/delete-by-focus-runtime-native.tsv) 与 [双击编辑 fixture](../verification/reaper-parity/fixtures/track-name-double-click-aaadaw.tsv)。

Nightly + Cranelift AAADAW 测试 551/551 passed；Stable workspace all-target Clippy `-D warnings`、fmt 和 diff 检查通过。`track_rename_is_undoable` 覆盖名称编辑、提交和可撤销行为。该修正只覆盖 TCP/MCP 名称控件；`INPUT-FOCUS-001` 继续 `in_progress`，完整文本控件、键盘布局、插件和子窗口焦点矩阵尚未验收。

### 2026-10-14 File 菜单 Quit 与 Ctrl+Q

REAPER 7.82 File 末项是 Quit，Action List 显示 `File: Quit REAPER` / Ctrl+Q（40004）。AAADAW 现在把该动作注册为可配置快捷键和 File 菜单末项；执行经统一主窗口关闭请求，因此未保存工程提供 Save、Don't Save、Cancel，取消或出现提示时不会丢失工程。File 菜单 ArrowUp 边界现在选中 Quit，实窗截图确认了菜单选择与 Ctrl+Q 触发的未保存工程确认框，证据及 SHA-256 见 [AAADAW 菜单边界 fixture](../verification/reaper-parity/fixtures/menu-keyboard-boundary-aaadaw.tsv)。

Nightly + Cranelift 定向回归 `quit_matches_native_file_menu_and_uses_dirty_close_confirmation` 1/1 passed，覆盖末项/标签/分隔线/Ctrl+Q 默认值及取消/放弃流程。File 菜单的其它原厂条目和完整键盘焦点行为尚未复刻；`SHELL-MENU-KEYBOARD-001` 继续 `in_progress`。

### 2026-10-14 File 菜单 Close project 与 Ctrl+F4

REAPER File 菜单的 `Close project` 对应 Main Action 40860 `Close current project tab` / Ctrl+F4。AAADAW 现在注册该可配置动作；对已修改工程使用统一 Save / Don't Save / Cancel 转换，确认关闭后清空当前工程、路径、选择及修订状态并创建空工程工作区。实窗菜单顺序、Ctrl+F4 未保存确认截图和 SHA-256 见 [Close project fixture](../verification/reaper-parity/fixtures/menu-close-project-aaadaw.tsv)。

Nightly + Cranelift 定向回归 `close_project_matches_native_menu_and_preserves_unsaved_work_until_confirmed` 与 `successful_save_before_close_project_clears_the_open_project` 2/2 passed，覆盖菜单、Ctrl+F4 默认绑定、Action List 原厂动作名、取消保留、Save 后关闭及 Don't Save 关闭。此行为只支持单工程；REAPER 多标签及 Close all projects 仍未实现，`SHELL-MENU-KEYBOARD-001` 保持 `in_progress`。

### 2026-10-14 File 菜单 Save project as Ctrl+Alt+S

REAPER Main Action 40022 的原厂名称为 `File: Save project as...`，默认 Ctrl+Alt+S。AAADAW 将 File 菜单标签的省略号和组合键补齐；快捷键使用现有 SaveProjectAs/原生文件选择器路径。实窗 File 菜单截图显示保存、关闭和退出命令及各自快捷键，SHA-256 见 [Save project as fixture](../verification/reaper-parity/fixtures/menu-save-project-as-aaadaw.tsv)。

Nightly + Cranelift 定向回归 `save_project_as_matches_native_ctrl_alt_s_and_opens_the_save_picker` 1/1 passed，验证 Ctrl+Alt+S 解码为 SaveProjectAs、打开保存选择器状态并关闭菜单。Xvfb 中未捕获到可见的原生文件选择器窗口，故只验证菜单截图和应用内路由，不宣称选择器视觉一致。File 菜单及键盘焦点仍有其它未实现命令，`SHELL-MENU-KEYBOARD-001` 保持 `in_progress`。

### 2026-10-14 File 菜单 Save new version of project

REAPER Main action 41895 在 File 菜单显示 `Save new version of project` / Ctrl+Alt+Shift+S，Action List 原名为 `File: Save new version of project (automatically increment project name)`。Linux REAPER 7.82 / Default 7 原厂探针确认，连续两次执行会从 `save-version-probe.rpp` 生成 `_001`、`_002` 并把活动工程切到新路径。AAADAW 为已有工程实现同样的递增命名，跳过已占用版本路径，复用原子保存、媒体复制和会话锁；成功保存才切换活动路径。未命名工程进入保存选择器。

原厂与 AAADAW File 菜单实窗截图及哈希、探针输入/输出、环境和验收边界见 [Save new version fixture](../verification/reaper-parity/fixtures/menu-save-new-version-aaadaw.tsv)。四项回归覆盖 Action List 分类/名称和快捷键、菜单显示、派发、编号冲突与来源工程保留。最终源码上的 Nightly + Cranelift 定向 nextest 4/4、AAADAW app 目标全量 559/559 通过；Stable AAADAW Clippy `-D warnings`、fmt、diff 和矩阵结构检查也通过。`SHELL-MENU-KEYBOARD-001` 仍为 `in_progress`：File 的其余命令、完整 Action List 窗口几何、所有工程命名边界及整体菜单/界面一致性未完成。

### 2026-10-14 File 菜单 Save all projects 与 Close all projects

Linux REAPER 7.82 / Default 7 的 File 菜单与 Action List 包含无默认快捷键的 Main actions 40897 `File: Save all projects` 和 40886 `File: Close all projects`。原厂探针在唯一已保存工程中增加一条轨道后运行 Save all，确认活动工程路径未变且 RPP 已保存轨道；再运行 Close all，当前路径清空、轨道数归零并回到空白工程工作区。AAADAW 按原厂菜单次序加入两个独立动作；单工程下 Save all 保存当前工程，Close all 复用已有未保存变更保护并关闭当前工程。原厂探针输出、原厂/AAADAW 菜单截图、哈希和验收范围见 [Save/Close all fixture](../verification/reaper-parity/fixtures/menu-save-close-all-aaadaw.tsv)。

Nightly + Cranelift 三项定向 nextest 3/3、最终源码上的 AAADAW app 目标全量 562/562 通过；Stable AAADAW Clippy `-D warnings`、fmt、diff 和矩阵结构检查通过。实窗 File 菜单截图确认新条目与分隔线顺序。单项目保护行为与原厂空白工程结果一致；多标签下真正保存/关闭所有项目仍待多工程模型实现，`SHELL-MENU-KEYBOARD-001` 保持 `in_progress`。

### 2026-10-14 File 菜单 Render... 与 Ctrl+Alt+R

原厂 Main action 40015 的 Action List 名称是 `File: Render project to disk...` / Ctrl+Alt+R，File 菜单短标签是 `Render...`。AAADAW 现将此动作映射至现有渲染窗口，并更新默认组合键、Action List 名称和菜单短标签；窗口仍显示并输出当前实现支持的 WAV。原厂与 AAADAW File 菜单实窗图、哈希、环境和对照范围见 [Render action fixture](../verification/reaper-parity/fixtures/menu-render-action-aaadaw.tsv)。

`wav_render_command_is_discoverable_queueable_and_cancellable_from_the_file_menu` 定向回归覆盖菜单标签、Action List 名称/分类、Ctrl+Alt+R 默认值与实际解析，并继续验证渲染/排队中可取消及录音期间禁用状态。最终源码上的 Nightly + Cranelift 定向测试 1/1、AAADAW app 目标全量 562/562 通过；Stable AAADAW Clippy `-D warnings`、fmt、diff 和矩阵结构检查通过。REAPER 的非 WAV 编码、完整 Render to File 面板及整体视觉对齐仍未实现，`SHELL-MENU-KEYBOARD-001` 和 Render 原子项继续 `in_progress`。

### 2026-10-14 File 菜单 Queued Renders

原厂 Main action 40929 的 Action List 名称为 `File: Open render queue`，无默认快捷键，File 菜单显示 `Queued Renders`。AAADAW 增加独立动作并复用现有 Offline Jobs 面板；Open 操作幂等，不会像 View 菜单 Toggle Offline Jobs 那样关闭已打开的面板。原厂/AAADAW File 菜单截图、哈希、环境和范围见 [Render queue fixture](../verification/reaper-parity/fixtures/menu-render-queue-aaadaw.tsv)。

Nightly + Cranelift 定向回归 2/2、最终源码上的 AAADAW app 目标全量 563/563 通过；Stable AAADAW Clippy `-D warnings`、fmt、diff 和矩阵结构检查通过。回归验证菜单顺序、Action List 分类/名称、无快捷键、Action List 搜索唯一命中，以及连续调用后面板仍打开。原厂 Render Queue 对话框与 AAADAW Offline Jobs 面板布局/操作还有差异，完整 File 菜单与窗口视觉继续为 `in_progress`。

### 2026-10-14 File 菜单 Export project MIDI...

Linux REAPER 7.82 原厂对话框探针确认 Main action 40849、无快捷键，以及 Type 1 / PPQN 960 默认值；tempo/meter、SMPTE 和 marker 导出默认关闭。AAADAW 已实现对应的导出设置窗口与 SMF 编码：Type 0/1、PPQN 转换、可选速度/拍号/SMPTE、marker/cue 和 `#` 筛选；文件写入在后台使用临时文件原子替换。缩放 PPQN 时会保留至少一个导出 tick 的正音符时长，避免 Note Off 排在 Note On 前导致挂音。原厂与 AAADAW 对照截图及未完成边界见 [Export MIDI 验收记录](../verification/reaper-parity/menu-export-midi.md)。核心编码回归 6/6、AAADAW action app 回归 1/1 通过；桌面二进制也已用本地 pkg-config/linker 条目链接并在 Xvfb 下运行。实窗默认 Type 1/PPQN 960 导出得到 39 字节、1 轨的 format-1 MIDI 文件；窗口截图和文件哈希均已登记。

通过仓库内 ignored `target/` 下的本地 pkg-config/linker 条目补足 GLib 开发元数据后，新的桌面二进制已成功链接并在 Xvfb `:124` 上持续运行；系统包未改动。截图验证确认实窗 File 菜单可打开 MIDI 导出窗口，窗口实际为 520×298，与原厂尺寸吻合；默认 Type 1/PPQN 960，marker 模式和筛选控件在空工程中正确禁用。实窗默认导出再次生成可识别的 format-1、单轨、39 字节 MIDI，SHA-256 与之前结果相同。更新后的菜单、对话框和导出成功截图及哈希记录在 [验收记录](../verification/reaper-parity/menu-export-midi.md)。`cargo test -p aaadaw --bin aaadaw export` 通过 34 项，其中包含两个 MIDI 界面回归；核心编码测试 6/6 通过，稳定版 Clippy `-D warnings`、rustfmt 和 diff 检查也通过。

对照截图显示剩余界面差异：AAADAW 使用深色主题，控件样式仍与原厂不同；File 菜单命令顺序和条目覆盖也不完整。项目模型没有 hidden-marker 状态，因此该控件保持禁用；原厂生成 MIDI 的逐事件比较、焦点/全量设置交互及其余 REAPER 界面仍未完成。`FILE-MIDI-EXPORT-001` 与 `SHELL-MENU-KEYBOARD-001` 继续保持 `in_progress`。

### 2026-10-14 Item selection actions now match native Undo behavior

Linux REAPER 7.82 / Default 7 native window probe exercised Main actions 40182, 40421, 40717, 40718 and 40289 on a disposable 3-track fixture containing 13 Audio/MIDI items. All five state-changing selection actions increment native project state count once and expose the expected Undo labels. Action 40421 includes every selected track regardless of selection order; time-selection actions use half-open overlap at both boundaries; Unselect preserves selected track, time range and edit cursor, and its Undo restores the prior item selection. Repeating Unselect on an empty selection produces no state change. A changed selection action clears an existing redo branch.

The covered Action List selection commands and hover-target selection actions now record item-selection snapshots in application-level Undo/Redo history interleaved with project edits; they leave the project content revision unchanged, restore captured item/active-item/track-anchor/automation-point selections, and discard project redo history only when selection changes. Action 40421 now matches native multi-track scope. New tests cover project/selection ordering, no-op redo preservation, redo-branch invalidation and multi-track scope. Stable AAADAW app tests passed 568/568 (selection filter 8/8); Stable app-bin/test Clippy with `-D warnings`, rustfmt, diff check, matrix shape, evidence paths, and recorded image hashes passed. Native REAPER artifacts are [the Arrange capture](../verification/reaper-parity/fixtures/item-selection-all-native.png), [the action probe TSV](../verification/reaper-parity/fixtures/item-selection-actions-native.tsv), and [the disposable probe script](../../scripts/reaper_parity/probe-item-selection-actions.lua). A rebuilt AAADAW desktop binary was also run against a copy of the two-track MIDI fixture at 1280×1024: screenshots show Action List search, both selected items, Undo, and Redo; hashes and binary Build ID are recorded in [the AAADAW verification manifest](../verification/reaper-parity/fixtures/item-selection-aaadaw.tsv).

The native probe did not verify saved-project persistence, dirty indication, automation-point interaction, empty-project enablement, keyboard focus, or all five AAADAW live-window states. Those limits remain open in `ITEM-SELECT-001` through `ITEM-SELECT-005`; the actions are not marked complete. Only 40182 has a complete real-window Action List → Select all → Undo → Redo capture, so the other four live action states remain open. UI theme parity and native persistence/dirty-state semantics remain open. This pass used the existing Stable build cache; a separate Cranelift profile cache was not rebuilt. Continue with the highest-impact remaining gap in the parity matrix.

### 2026-10-14 Project Marker 可见性与真实桌面运行恢复

Linux REAPER 7.82 原厂探针确认单 Marker Visible 与整个 Marker 标尺显示是两个独立状态，且 `Export hidden markers` 会控制隐藏 Marker 是否写入 MIDI。AAADAW 已将两类可见性接入可撤销项目动作、快照和 SQLite schema 47；Manager 增加单项 Visible 控件，View 菜单增加有勾选状态的全局标尺动作，MIDI 导出控件按 Marker 存在及 Marker 导出开关动态启用并默认过滤隐藏项。时间线绘制及命中也尊重两种隐藏状态。

新桌面二进制此前因容器只有 GLib 运行库、缺少开发 `.pc` 元数据而不能链接。本轮在 ignored `target/` 中创建本地 pkg-config 描述和动态库链接路径，没有改系统包；随后在 Xvfb `:125` 实际运行新二进制并截取 Manager 单项显示/隐藏、标尺旗标显示/隐藏和 View 菜单截图。二进制、截图 SHA-256、运行环境及验证边界记录在 [AAADAW 可见性运行清单](../verification/reaper-parity/fixtures/marker-visibility-aaadaw.tsv)；原厂 7.82 探针在 [全部标尺状态](../verification/reaper-parity/fixtures/marker-visibility-native.tsv)、[选中 Marker 状态](../verification/reaper-parity/fixtures/marker-visibility-selected-native.tsv)及 [验证说明](../verification/reaper-parity/project-markers.md)。

`cargo check -p aaadaw --no-default-features --tests`、fmt 和 diff 检查通过；Core Marker/MIDI 定向测试 12/12、schema 可见性存储测试 4/4 通过。由于当前容器剩余磁盘约 1.2 GB，本轮没有再次链接庞大的 app 测试二进制；app 可见性测试已通过类型检查，并以实窗截图验证状态变化。`PROJECT-MARKERS-001`、`FILE-MIDI-EXPORT-001`、UI-013 和 P4 继续 `in_progress`：原厂 action 43484 的多选批量切换及 Manager/Region 其余布局与功能、更多 Marker 时间基准/编辑语义和完整主题对照尚未完成。

### 2026-10-14 Action 43484：批量显示/隐藏所选 Marker

Linux REAPER 7.82 / Default 7 实窗中，先把一个所选 Marker 设为隐藏、另一个保持可见，再从 Action List 执行 Main action 43484：两个所选 Marker 都显示；一次 Ctrl+Z 将其恢复为原混合状态。AAADAW 现注册 `Ruler: Show/hide selected project markers`（稳定 ID `ruler.toggle-selected-project-markers`），使用 Manager 多选；只要所选集合中有隐藏 Marker 就统一显示，否则统一隐藏。未选 Marker 不变，无有效选择或工程忙碌时动作禁用，多项改动写为单一事务。

新构建 AAADAW 二进制以 Xvfb 实窗操作 Manager 的 Ctrl 多选、Action List 运行、混合状态显示、单步 Undo/Redo，以及全可见集合隐藏。原厂工程与截图、AAADAW 截图、二进制/图片哈希和对照范围见 [多选 Marker 可见性 fixture](../verification/reaper-parity/fixtures/marker-selected-visibility-multiselect.tsv)。

`cargo check -p aaadaw --no-default-features --tests` 与 AAADAW app/test Clippy `-D warnings`、fmt、diff 检查通过；本轮 app 测试源码已类型检查，实窗完成上述交互验证，未重新链接 app 测试套件。`PROJECT-MARKERS-001`、UI-013 与 P4 继续 `in_progress`：Manager 的 Ruler Lane、Render Matrix、Lane Manager、Options 和完整布局/主题、同位置 Marker 行为、负时间/timebase、Region 全套交互与其它项目语义仍待实现和原厂验证。

### 2026-10-14 Project Region 可见性与桌面运行恢复

在同一 P4 分支上，Project Region 现在保存独立的单项 `Visible` 和整个 Region 标尺显示状态。两者进入可撤销 Core action、快照和 SQLite schema 48；schema 47 工程迁移后均默认显示。Region/Marker Manager 的 Regions 行显示 Visible 复选框，View 菜单加入 Region 标尺显隐项，Action List 注册所选 Region、全部 Region 及 Region/Marker 组合动作。Arrange 绘制和命中测试跳过隐藏标尺或隐藏 Region。所选 Region 的多项改变合并为一个事务。

Linux REAPER 7.82 / Default 7 原厂实窗验证 action 43486 切换所选 Region，并以一次 Ctrl+Z 恢复；43487 隐藏 Region 标尺；43489 在任一标尺隐藏时显示两者、两者均显示时隐藏两者。原厂 Action List 与 Manager 截图、工程状态及哈希见[原厂记录](../verification/reaper-parity/fixtures/region-visibility-native.tsv)。新 AAADAW 桌面二进制通过 ignored `target/runtime-pkgconfig` 与 `target/runtime-libs` 下的 GLib 链接元数据成功构建并在独立 Xvfb `:134` 启动；未改系统包。实窗验证了 Region Visible 状态、所选 Region Action List 动作、隐藏→显示、Undo 隐藏、Redo 显示，且保存了 Arrange、View 菜单、Manager、Action List 与历史状态截图。二进制 Build ID、SHA-256、各截图哈希和具体范围见[AAADAW 运行清单](../verification/reaper-parity/fixtures/region-visibility-aaadaw.tsv)。

Core 可见性/Undo/快照测试 1/1、Storage 往返及 schema 36→37 Region 迁移测试 1/1、schema 47→48 可见性默认迁移测试 1/1 通过。Core/Storage check、AAADAW app/test 类型检查、AAADAW Clippy `-D warnings`、新桌面二进制构建、fmt 和 `git diff --check` 通过。App 测试链接曾启动，但可用空间下降到约 100 MiB 时中止，未生成或运行测试套件；随后 `cargo clean -p aaadaw` 回收 1.7 GiB，并重新构建、启动同哈希和 Build ID 的桌面二进制。运行清单如实记录此限制。`PROJECT-REGIONS-001`、`PROJECT-MARKERS-001`、UI-013 和 P4 继续 `in_progress`：AAADAW 全局 Regions+Markers 标尺动作实窗运行、完整属性/Manager、时间基准、边界手势和主题验收仍未完成。

### 2026-10-14 Action 43488：混合 Region/Marker 可见性

Linux REAPER 7.82 / Default 7 的 Region/Marker Manager 中 Ctrl-选择隐藏的 R1 Region 与可见的 M1 Marker，再从 Action List 执行 Main action 43488。原厂将两项统一设为可见；单次 Ctrl+Z 恢复“R1 隐藏、M1 可见”，Main action 40030 Redo 再次统一显示，未选 M2/M3 状态保持不变。筛选后的 Action List、混合选择、执行、Undo、Redo 截图及保存的 `.rpp` 状态均有 SHA-256，见[原厂可见性探针](../verification/reaper-parity/fixtures/region-visibility-native.tsv)。

该结果支持 AAADAW 已实现的“选中集合中有隐藏项就全部显示，否则全部隐藏”规则。AAADAW 新构建桌面版也在同一混合工程中运行该 Action List 命令，实测两项统一显示、Ctrl+Z 恢复混合状态、Ctrl+Shift+Z 再次显示；实窗截图、Build ID、SHA-256 与工程样例见[AAADAW 组合动作运行清单](../verification/reaper-parity/fixtures/region-visibility-selected-combined-aaadaw.tsv)。新增 App 回归补上混合 Region/Marker 集合的 Redo 断言。此次只实测一种混合选择组合，不将其泛化为所有选择顺序与 Lane 状态。App 测试链接仍受磁盘约束未运行，Clippy `-D warnings` 和类型检查通过；AAADAW 全局 Regions+Markers 标尺动作仍待实窗验证。`PROJECT-REGIONS-001`、UI-013 和 P4 继续 `in_progress`。

### 2026-10-14 Action 43489：AAADAW 全局 Region/Marker 标尺显隐

在 Linux AAADAW 新桌面二进制中，使用同一个 Region/Marker fixture 操作 Action List 的 `ruler.toggle-project-regions-and-markers`。Region 标尺隐藏、Marker 标尺显示时执行动作，两条标尺均显示；Ctrl+Z 恢复混合状态；Ctrl+Shift+Z 再次显示两者。随后从两者可见状态执行动作，两条标尺均隐藏；Ctrl+Z 恢复两者可见，Ctrl+Shift+Z 再次隐藏。Action List、混合状态、显示、Undo、隐藏与 Redo 截图及 binary hash/Build ID 见[全局组合动作运行清单](../verification/reaper-parity/fixtures/region-visibility-global-combined-aaadaw.tsv)。该状态转换与 Linux REAPER 7.82 Main action 43489 的实窗结果一致。

App 测试 `region_lane_view_actions_match_mixed_lane_undo_and_redo` 现覆盖混合 Lane 状态下组合动作的显示、Undo/Redo，也覆盖两者可见时一次动作隐藏两者。测试源码已通过 AAADAW app/test Clippy `-D warnings` 和类型检查；app 测试链接因容器磁盘空间不足未生成测试二进制，未运行。此前记录“全局组合动作尚待实窗验证”现由本次结果更新；Region Manager 字段/布局、边界手势、ID 分配、时间格式、循环/Repeat、Native 项目互操作与完整主题几何仍未验收，`PROJECT-REGIONS-001`、`PROJECT-MARKERS-001`、UI-013 和 P4 保持 `in_progress`。

### 2026-10-14 Region 属性窗 Position/Length 与 Manager 入口

AAADAW Edit Region 窗口现在使用 REAPER 属性窗中的 Position 与 Length 字段，而不是分别编辑 Start/End。两字段使用当前 Bars.Beats（`bar.beat.hundredths`）格式，Length 按编辑后的 Position 锚定到项目 Tempo/Meter，再转换为 Core 保存的边界 ticks；无效、零长度或越界长度保留草稿且不写项目。增加 `Region Manager...` 入口；打开 Manager 后属性草稿窗仍保持存在。Core/SQLite schema 不变，实时音频 callback 无改动。

Linux AAADAW 新桌面版实窗显示 Name、ID、Position、Length（示例 `1.1.00` / `0.2.00`）。点击 `Region Manager...` 后 Manager 窗口打开，属性窗口仍存在于 X11 窗口树中。两张实窗截图、binary Build ID/SHA-256 与 fixture 项目哈希见[属性窗运行清单](../verification/reaper-parity/fixtures/region-properties-aaadaw.tsv)；未把输入模拟未完成的试验记作文本编辑成功证据。

`region_manager_selects_edits_filters_and_deletes_with_undo` 覆盖 Position/Length 初值、修改后单步提交/Undo 与属性窗打开 Manager；属性测试覆盖 Manager 启动、零长度拒绝与 Cancel。完整 App 测试套件 572/572 通过，app/test strict Clippy、fmt 和 `git diff --check` 通过。Lane 数值、Region Render Matrix、Set/Reset color、其它时间格式/负时间、窗口其余控件与 Default 7 视觉仍未验收；`PROJECT-REGIONS-001` 和 P4 继续 `in_progress`。

### 2026-10-14 桌面二进制 Xvfb 运行恢复与 Nudge 文档校正

此前重启 AAADAW 时已没有有效 `DISPLAY`，系统也没有安装 Xvfb；旧 Xvfb/应用进程多数已退出为僵尸进程。改用隔离 REAPER 运行包中的 Xvfb 创建新显示后，最新 AAADAW 二进制 SHA-256 `59bc8bfa475f11edc45e8eff911e63b1b003148b481ffbf07838ea4d58b86e44` 正常显示 1280×800 主窗口。用 XTest 点击 Item 菜单与 `Nudge/set items...`，真实窗口树报告 528×121 Nudge/Set 窗口。新增可复现启动/首帧截图脚本 `scripts/reaper_parity/run-aaadaw-desktop-smoke.sh`，它隔离 XDG profile、自动选用系统或 REAPER 包内 Xvfb，并轮询实际非黑色帧；运行截图、交互截图及 SHA-256 见[桌面运行记录](../verification/reaper-parity/desktop-runtime.md)。

复核 `ITEM-NUDGE-SET-001` 时发现 bounded source loop wrap 已有实现与 App 测试，但详细说明仍称其未实现。已更正 Nudge 主文档、Item 菜单对照和 fixture 状态；当前明确区分“AAADAW 实现/单测通过”和“REAPER 原生边界行为未复验”，该项仍 `in_progress`。启动脚本实际运行成功；bash 语法检查、截图窗口/像素检查和 `git diff --check` 通过。下一步回到原厂 Nudge 行为探测，不以本次桌面启动证明代替具体功能验收。

### 2026-10-14 Item Contents Nudge 源边界与桌面运行复验

提交 `65eb38c481e526f30ad52c2df4cd828f3e1788f1`（矩阵 `ITEM-NUDGE-SET-001`）扩展 Linux REAPER 7.82 / Default 7 原厂探针，实测整份 10 秒循环源从 0.1 秒向右 Nudge 后绕回 9.85 秒、从 9.9 秒向左 Nudge 后绕回 0.15 秒；非循环源两侧则得到 −0.15 秒与 10.15 秒。四种操作都保持 timeline position=2 秒、item length=10 秒，REAPER project state-change 增量为 0。探针 TSV 与非循环边界实窗截图已归档。

AAADAW Contents Nudge 现在允许正向越过源 EOF。媒体 feeder 在源数据结束后以固定 1024 帧静音块填充有限 item 的剩余长度，保持有界队列回压；实时 callback 未改动。Core 的源偏移仍为 `u64`，SQLite 对该字段仍有非负约束，因此原厂允许的负偏移尚未实现，保持明确差异。显式 bounded source section 的原厂行为也未验收。

`cargo xtest` 全工作区在临时 Xvfb :97 与 nightly Cranelift 下 1108/1108 通过；定向 media EOF 测试 2/2、App Nudge/Undo 测试 1/1、media/App Clippy `-D warnings`、fmt 和 `git diff --check` 通过。更新后的 `target/debug/aaadaw` Build ID `969fe3e9997570899c03c004f8c2941506c012e4`、SHA-256 `7410ee3a4d34bfc579f09f941df3aafef4b168ff68032c2b613de53e715809fe` 经桌面烟测显示为 1280×800 窗口，截图 hash 与已存启动图相同；完整记录见 [桌面运行验证](../verification/reaper-parity/desktop-runtime.md)。

本轮已完成下方记录的 signed source offset Core/SQLite/Media 实现及负偏移回归。之后继续探测 bounded-section、MIDI target 与 Set/Snap 行为；`ITEM-NUDGE-SET-001`、P4 与唯一 PR #282 保持进行中。

### 2026-10-14 Signed source offset 全链路与新桌面二进制

实现提交：`0a8776e94df6a6178d4716b4b5966d071fa524ef`（矩阵 `ITEM-NUDGE-SET-001`）。

将非循环 Audio Item 的源偏移扩为 signed `i64`，覆盖 Core action/snapshot、导入替换、Nudge、Trim/Split、属性编辑及 waveform peak 映射。负偏移令新插入项默认关闭 loop source；试图打开循环或设置无效 bounded section 会被 Core 拒绝。Media feeder 在线程内用固定 1024 帧静音块补足源起点前的 pre-roll，并将 seek 位置正确拆分为未播放静音和源帧跳过；EOF 后仍补静音至 item 长度。音频实时 callback 没有改动。

SQLite schema 升至 49，新增权威 `signed_source_offset_samples`，旧非负列保留作兼容镜像；v48 偏移原样复制，读写重开保留负值。新增字段没有跨版本 loop 引用 CHECK，以兼容旧 schema 的 loop 列删除/添加迁移顺序，负循环偏移由 Core 项目恢复和 action 校验拒绝。迁移回滚规则为恢复旧数据库备份，不手工降 `user_version`。详细结构与回滚方式见[存储变更记录](domain-and-storage-changes.md#schema-49--signed-audio-source-offset)。

Linux REAPER 7.82 已有探针记录非循环源负偏移值 −0.15 秒；本轮 AAADAW 回归验证了负偏移模型/snapshot、feed 静音、pre-roll 中间 seek、SQLite v48→49 与保存重开、Item Properties 负值编辑和 waveform 在 pre-roll 后显示。Core/Media/Storage 测试 245 项、App 测试 89 项、AAADAW 桌面单测 574 项、Engine 测试（Xvfb :97、单线程）185 项全部通过。全目标 Clippy `-D warnings`、fmt 与 diff 检查通过。

最新桌面构建 Build ID `e1ab6146acaad9e9ea2d808e1f3261199529c792`、SHA-256 `0cdb64ab78668b631e6cec8b5b6a97db3f69387de364ee496fcf2a65fcfaa105` 已成功在 Xvfb :90 启动并绘制 1280×800 主窗口；新截图和哈希见[运行清单](../verification/reaper-parity/desktop-runtime.md)。这是运行恢复截图，不是负偏移属性控件的专项实窗截图；该编辑路径目前有单测覆盖，窗口级像素/焦点验收仍待完成。

`ITEM-NUDGE-SET-001`、P4 与 PR #282 继续 `in_progress`。后续探测 bounded source section 在 loop 关闭状态下的原厂偏移边界，再做负偏移属性窗实窗交互/截图，以及 MIDI target、Set/Snap、多选锚点与 Undo 语义；不得把本次运行烟测当作完整 UI 验收。

### 2026-10-14 Bounded source section 的偏移坐标与 Contents Nudge

Linux REAPER 7.82 / Default 7 原厂探针补齐了 bounded source section 的 loop-off 行为：源段为 `[0, 3]` 或 `[3, 6]` 秒时，`D_STARTOFFS=0.1` 执行 250 ms Contents Nudge 右移都变为 `-0.15` 秒；段起点、段长、loop-off、item 的 12 秒位置与 3 秒长度均保持不变，project state-change 增量为 0。探针脚本、状态 TSV 与实际 Nudge/Set 窗截图见[验证记录](../verification/reaper-parity/item-nudge-bounded-section.md)。这确认 section 起点使用媒体绝对帧，`D_STARTOFFS` 使用段内相对帧。

AAADAW 现在将这两个坐标分开贯穿 Core 校验、Media feeder、循环复制、Contents Nudge、waveform 映射与 Storage。关闭 loop 的 bounded section 允许 signed 段内偏移：负值先输出 pre-roll 静音，再从段首单次播放并在段尾补静音；loop-on 的偏移仍必须非负并在段内绕回。SQLite schema 升至 50，把 schema 49 旧 bounded 绝对偏移转换为段内偏移；旧非负列仍是 signed 权威列的兼容镜像。实时音频 callback 未改动。

Core audio-item editing 9/9、Media WAV feeder 21/21、Storage bounded-section migration/round-trip 2/2、AAADAW Contents Nudge 13/13、循环复制、signed pre-roll waveform 测试各 1/1 通过；最终源码上的 Core/Media/Storage/AAADAW all-target Clippy `-D warnings` 通过。Stable 桌面二进制重新构建后在隔离 Xvfb `:90` 启动，真实主窗口为 1280×800；构建 ID、二进制哈希和与启动 fixture 像素相同的截图哈希已写入[运行清单](../verification/reaper-parity/desktop-runtime.md)。

REAPER 负偏移的逐采样音频比较尚未验证；Nudge 的其他 Target/Snap、MIDI、多选锚点、选择恢复、Undo 完整语义和对话框视觉/滚动对齐仍开放。`ITEM-NUDGE-SET-001`、P4 与唯一 PR #282 继续 `in_progress`。

### 2026-10-14 REAPER MIDI Contents Nudge 原厂探测

Linux REAPER 7.82 / Default 7 原厂实窗探针使用五秒循环 MIDI item、两条带 PPQ 标记的音符和 250 ms Nudge。Contents 目标下，41249 将 `D_STARTOFFS` 从 0 秒绕至 4.75 秒；41250 从 4.75 秒回到 0，再从 0 推进到 0.25 秒。六种 Snap 开/关状态的对应样例结果相同。整个过程中 item 位置 2 秒、长度 5 秒、loop flag 和选择保持不变，原始 MIDI note PPQ 范围不变，project state-change 计数增量为 0。实测值、截图和脚本见[MIDI Contents Nudge 记录](../verification/reaper-parity/item-nudge-midi-contents.md)。

AAADAW 当前对 MIDI Contents Nudge fail closed。现有 MIDI `source_offset_ticks` 不保存可用于环绕的源循环长度，因此只改偏移会在 timeline、MIDI editor、playback 或 MIDI export 中丢失循环片段。Undo_CanUndo2 标签在本轮 probe 中不稳定，音频设备模态窗口也阻挡了独立 Ctrl+Z 验证，故 Undo 语义仍未判定。下一步先扩展 MIDI source loop 数据模型，再把 loop 长度贯穿保存、编辑显示、播放、导出、剪切/复制和 Undo，并以这些原厂值建立回归；`ITEM-NUDGE-SET-001` 继续 `in_progress`。

### 2026-10-14 MIDI source loop 与 Contents Nudge

实现推进：schema 51 在 MIDI item 上增加 `loop_source` 与固定 `source_loop_length_ticks`；新 MIDI item 默认沿用 REAPER MIDI item 的 loop-source 行为，旧工程迁移按 item 长度和事件最大结束位置推断 source length。Duplicate、Split、Snapshot 与 SQLite 保存/重开会保留该元数据。对旧 trimmed empty MIDI items 无法恢复历史源长度，推断值仍是已知兼容边界。

Core 现在提供无分配的 source-event loop occurrence iterator；离线/实时共享的 MIDI 事件计划、Arrangement note preview 和标准 MIDI 导出会按可见 item 长度重复 note/controller/pitch-bend。事件计划与导出各有 1,000,000 MIDI event 的硬上限，避免极短 loop 扩展成失控内存。Contents Nudge 对单独选中的 looped MIDI item 按每个 item 起点所在的 Tempo Map 把毫秒时长换算成 PPQ tick，再环绕源偏移；250 ms / 120 BPM 用例从 0 到 9120 tick（4.75 秒），Undo 恢复 0，向左移动到 480 tick（0.25 秒）。新增 item 位于 60 BPM tempo point 后的回归确认 250 ms 转为 240 ticks。混合 Audio/MIDI 选择、非循环 MIDI source、Snap 和 Set 继续失败关闭；REAPER 原厂变速工程的逐项对照仍待验证。

定向回归：Core loop mapping 与 MIDI export 9/9、Storage loop/migration 10/10、Engine MIDI schedule 15/15、新的 Tempo Map app 回归 1/1；严格 all-target Clippy `-D warnings`、fmt、diff 检查通过。Linux desktop 使用新建的 [`build-and-smoke-aaadaw-desktop.sh`](../../scripts/reaper_parity/build-and-smoke-aaadaw-desktop.sh) 重建并在 Xvfb 运行，实际主窗口 1280×800；SHA 与启动 fixture 相同。为避免窄磁盘工作区在 GUI 二进制重链接时失败，该脚本对最终桌面可执行文件剥离 DWARF，并仅在可用空间低于 1 GiB 时清理 AAADAW 自身增量缓存与失败链接临时对象；Rust 依赖 debug 信息和运行代码生成保持不变。新二进制和截图哈希见[桌面运行验证](../verification/reaper-parity/desktop-runtime.md)。该启动截图不覆盖 Nudge 功能控件。

`ITEM-NUDGE-SET-001` 继续 `in_progress`：MIDI Editor 仍按未循环源坐标渲染/编辑；REAPER 变速工程换算对照、Undo 语义、mixed-media、Snap/Set、循环源 UI、bounded MIDI source 和全部目标截图仍待完成；不能把此纵向数据/播放/导出切片视为 Nudge 或 P7 完成。


### 2026-10-15 MIDI Editor loop-source 实例显示与源事件编辑

提交：`d3781a44b2c4c21f77ae1b3255ac7dc90e0b30e2`（`feat(midi-editor): edit looped source occurrences`）。

Linux REAPER 7.82 / Default 7 原厂探针确认：5 秒 MIDI source、10 秒 item、4.75 秒 source offset 下，MIDI Editor 显示 source note 在 clip-relative 0.75、2.75、5.75、7.75 秒的实例；脚本、TSV 和原厂截图见 [loop-source 验证](../verification/reaper-parity/midi-editor-loop-source.md)。AAADAW 实窗创建 4 拍 MIDI item、加入 C4、延长到 12 拍并打开 Piano roll，实际 1000×620 窗口截图显示同一音符在连续三个循环中出现。

钢琴卷帘绘制、选择、命中和拖动预览按可见 source loop occurrence 处理，操作仍指向共享源事件；新音符写入折回 loop-relative PPQ 并在循环接缝限长。CC 与 Pitch Bend lane 同样显示/命中重复实例，并将写入归一化为源刻度。Core note/controller/pitch-bend/quantize 校验改用 loop-source bounds，量化按 item position 映射回 source clock。

Core 全包测试 130/130（39 suites），AAADAW 桌面测试 579/579；其中 source-loop/controller/pitch-bend/quantize 回归 11 项、wrapped-offset MIDI Editor 测试 1/1、lane 回归 16/16。Core/AAADAW all-target Clippy `-D warnings`、cargo check、fmt/diff 通过。最终桌面构建 Build ID `108001ad22857fcc7f5ce81960203f5ec333c693`、SHA-256 `929def7e6e1d7835a25c89a726af78969f3f328a7d9f9f118a2fd80b69247401` 在 Xvfb :90 启动，实际 MIDI Editor 截图来自同一构建，完整哈希见验证文档。

循环实例显示/源事件基本编辑切片完成；截图采用零 source offset，非零偏移由 REAPER 实窗和 AAADAW 定向回归覆盖。用户可见的 loop/source-length 控件、MIDI Editor 全部手势/菜单的原厂对照、Contents Nudge 变速换算和 Undo 对齐仍未完成。`ITEM-NUDGE-SET-001`、P4、P7 与唯一 PR #282 继续 `in_progress`。

### 2026-10-15 Arrangement MIDI 长循环预览与桌面运行恢复

Arrangement 原先在 TimelineCache 中按 source note 顺序展开循环，并只保留前 512 个实例；长 MIDI item 后半段因此没有预览，较后的源音符也可能被前一条长循环音符挤掉。缓存现在保留源 note、trim offset 与 source-loop length，绘制时从当前屏幕可视 tick 区间直接计算相交实例，不从循环起点逐个遍历。每个 item 最多生成 512 个 note 矩形，并将预算分布到不同源音符及当前可见时间范围。

回归覆盖非零 source offset 下第 1,000 圈末端的实例、512 矩形上限的横向覆盖、trimmed 源事件保留与常规几何映射。AAADAW 桌面测试 581/581、Timeline 测试 79/79；App all-target Clippy `-D warnings`、fmt 与 diff 检查通过。验证细节见[Arrangement MIDI 循环预览](../verification/reaper-parity/arrangement-midi-loop-preview.md)。该功能状态截图及 Linux REAPER 7.82 原厂视图对照仍待采集，Arrangement parity 和 P4 不关闭。

本轮也解决了新桌面版无法启动的问题：当前会话 `DISPLAY` 未设置且 PATH 没有系统 Xvfb，直接依赖旧显示会话无法创建窗口。重建/烟测脚本从隔离 REAPER 运行包找到 `/tmp/aaadaw-reaper-782/root/usr/bin/Xvfb`，在新建 :90 显示实际启动 Build ID `b8b2910869dbd19460c75f02f1d4a93f59b6f218` 的 1280×800 主窗口；SHA 与启动截图哈希见[桌面运行验证](../verification/reaper-parity/desktop-runtime.md)。不需要安装系统包，也不依赖已有 `DISPLAY`。

### 2026-10-15 MIDI Contents Nudge 的 Snap 复选框对齐

REAPER 7.82 的 MIDI Contents Nudge 原厂探针已在 Snap 开/关两种状态下得到相同偏移值。AAADAW 原先在 Snap 开启时直接拒绝 Contents 目标；现在会执行已实现的 looped MIDI source-offset 换算，复选框不改变该目标的 250 ms 结果。新增 app 回归覆盖 Snap 开启时从 0 PPQ 偏移绕到 9120 PPQ，Tempo Map 转换及 Undo 原有测试仍通过。混合 Audio/MIDI 选择、非循环 MIDI source、Tempo Map 原厂变速对照、source-loop 控件和 Undo 原厂语义仍未完成。

最终 AAADAW 桌面套件 582/582、app all-target Clippy `-D warnings`、fmt/diff 检查通过。更新后的二进制 Build ID `5329ead90fcafe5b0426866078764d5ddcd17391`、SHA-256 `45c905a61634564c1cd66621abf9752207970e43e6d97acf78bc8c7e2e933e5e` 在隔离 Xvfb :90 显示真实 1280×800 主窗口，启动截图哈希与 fixture 相同；本次没有截取 Nudge 对话框功能状态图。原厂数值、功能范围和界限见[MIDI Contents Nudge 验证](../verification/reaper-parity/item-nudge-midi-contents.md)及[桌面运行记录](../verification/reaper-parity/desktop-runtime.md)。`ITEM-NUDGE-SET-001`、P4 与唯一 PR #282 继续 `in_progress`。

### 2026-10-15 混合 Audio/MIDI Contents Nudge 原子事务

Contents Nudge 现在会先分别校验并计算所有已选 Audio 与 MIDI item 的新 source offset，再把变更放进同一个 `BatchTransaction`。如果音频素材元数据、audio section 或 MIDI loop source 任一项无效，就整批拒绝，避免部分 item 已移动而另一些未执行。新回归覆盖 250 ms 下 Audio 源偏移 120→468120 帧、MIDI 源 PPQ 偏移 0→9120、时间线位置/长度不变，以及一次 Undo/Redo 同时恢复/重做两种 item；故意缺失 Audio 元数据的混选用例确认 MIDI 内容不会被单独修改。

最终桌面套件 583/583，混合选择与回滚专项 2/2；app all-target Clippy `-D warnings`、fmt/diff 检查通过。最新桌面 Build ID `9a2512681c66b00fdf688bae8136eeba7c03fd0b`、SHA-256 `3820f7490792e9e619d827b0d1d417b063ba80998c27d444f988425b253337ee` 在隔离 Xvfb :90 启动实际 1280×800 主窗口，启动截图与 fixture 一致。REAPER 7.82 对混合 Audio/MIDI Contents selection 的原厂结果仍需单独 probe；本地实现和测试不代替原厂验收，`ITEM-NUDGE-SET-001`、P4 与唯一 PR #282 继续 `in_progress`。

### 2026-10-15 REAPER 原厂混合 Contents Nudge 对照

在 Linux REAPER `7.82/linux-x86_64` / Default 7 的空白未保存工程中，同时选择一个 10 秒循环 Audio 源 item（item 2–7 秒、source offset 0.1 秒）和一个循环 MIDI item（同样 2–7 秒，notes 位于 PPQ 960–2880 和 4800–6720），通过原厂 Nudge/Set 窗设 Target `contents`、250 ms、Snap off。Main action 41249 将 Audio offset 环绕到 9.85 秒、MIDI offset 变为 4.75 秒；41250 将两者都还原到起始值。两次操作均保持选择、item 位置/长度与 MIDI note PPQ 不变。对 Left 操作执行一次 Undo 会同时恢复两种 item 的前一组 offsets，Redo 同时重做；该 probe 只验证这一个 Undo/Redo 周期，不据此推断更深历史的行为。原厂数值、state-change counter、截图哈希和脚本见[混合 Contents Nudge fixture](../verification/reaper-parity/fixtures/item-nudge-mixed-contents-native.tsv)及[验证说明](../verification/reaper-parity/item-nudge-midi-contents.md)。

### 2026-10-15 MIDI Contents Nudge 跨 stepped Tempo Point

REAPER 7.82 原厂工程以 120 BPM 开始，在 4.000 秒（第三小节第一拍）阶跃到 60 BPM；循环 MIDI item 从 3.900 秒开始、长 5 秒，因此 item 起点 100 ms 后跨过 Tempo Point。Target `contents`、250 ms、Snap off 时，item 起点到 +250 ms 的项目时间积分是 336 PPQ，但原厂 Main action 41249 将源相位移到 4416/4896 PPQ，即按 item 起点的 120 BPM 移动 480 PPQ。Main action 41250 到达 loop endpoint 4896 PPQ（与 phase 0 等价）；两条 MIDI note 的 PPQ、item bounds 与 selection 保持不变。完整前后态、D_STARTOFFS、TEMPO marker、截图 hash 和 [ReaScript](../../scripts/reaper_parity/probe-item-nudge-midi-variable-tempo.lua) 见[原厂 fixture](../verification/reaper-parity/fixtures/item-nudge-midi-variable-tempo-native.tsv)。

AAADAW 之前按“item start sample + duration”反查跨 Tempo Map 的 target tick，得到 336 PPQ，与原厂相差 144 PPQ。现在 MIDI Contents Nudge 用 item start tick 的 tempo 和项目 BPM basis 把 duration 换成 PPQ，并继续检查目标时间是否在项目支持范围内。新增 App seam 回归先复现 4560/4416 的失败，再验证 120 BPM item 起点跨到 60 BPM 时生成原厂预期 source offset 4416。nightly+Cranelift、测试调试信息关闭下，App 全测试 584/584、Nudge 定向 18/18 通过；Stable App all-target Clippy `-D warnings`、fmt 和 diff 检查通过。线性 tempo ramp、循环源 UI、旧 trimmed MIDI source 恢复与更广的 Undo 语义仍开放；`ITEM-NUDGE-SET-001`、P4 与唯一 PR #282 保持 `in_progress`。

### 2026-10-15 MIDI Item Properties Loop source

REAPER 7.82 / Default 7 原厂 `Media Item Properties` 窗口在新 MIDI item 上显示默认勾选的 `Loop source`，窗口树报告 526×693；实际 1280×800 截图和哈希见[原厂 fixture](../verification/reaper-parity/fixtures/midi-item-properties-loop-source-native.png)。AAADAW MIDI 属性窗此前缺少此控件，现在显示同名复选框并默认读取 item 状态。`SetMidiItemLoopSource` 用现有 MIDI item event 更新标志，不改 visible item length、source loop length 或 note 数据；播放期间属性 Apply 会拒绝该编辑，并将该动作纳入 playback graph refresh 路径。应用回归覆盖关/开经 Undo/Redo 且 source loop length 短于 visible item length，Engine 回归确认关闭后重复 MIDI 事件不再调度但 source length 保留。

AAADAW 实际 MIDI Properties 窗口树报告 526×390；原厂完整窗口还含本项目未实现的其它 take/fade 控件，所以本切片只验收 Loop source 的原厂状态、可见性与编辑行为。两张实窗截图、窗口尺寸、Build ID、二进制与截图哈希见[对照说明](../verification/reaper-parity/midi-item-properties-loop-source.md)及[fixture TSV](../verification/reaper-parity/fixtures/midi-item-properties-loop-source.tsv)。AAADAW App 全测试 585/585；Engine 全测试 187/187（bundled Xvfb）；Stable rustfmt/diff 检查通过。`ITEM-PROPERTIES-001`、`ITEM-NUDGE-SET-001`、P4 与唯一 PR #282 仍为 `in_progress`。

### 2026-10-15 MIDI Contents Nudge 线性 Tempo ramp 原厂值

新增 Linux REAPER 7.82 / Default 7 实窗探针：120 BPM 起始点启用 linear-to-next，在第三小节起点到达 60 BPM；原厂返回的 marker 时间是 5.333333 秒。选中 MIDI loop item 从 3.900 秒开始，Target `contents`、250 ms、Snap off。项目时间 250 ms 区间映射为 298.875 PPQ；41249 后源相位为 4537.985 / 4984.900 PPQ（向 loop 尾部绕回 446.915 PPQ），D_STARTOFFS 为 3.041994609 秒。41250 到达 4985.000 PPQ loop endpoint；item 边界、note PPQ、loop flag、selection 未变，直接动作 state-change 增量为 0。实窗前后截图、原厂 fixture 和[ReaScript](../../scripts/reaper_parity/probe-item-nudge-midi-linear-tempo.lua)见[线性 ramp 验证记录](../verification/reaper-parity/item-nudge-midi-linear-tempo.md)。AAADAW 现对 active Linear TempoCurve 使用 source-time 映射；回归 phase 为 4531 PPQ，与原厂相差 6.985 PPQ，原因及 loop 长度量化界限已记录，精确 source-loop/timebase 对齐仍开放。

### 2026-10-15 MIDI Contents Nudge stepped boundary 对照与线性 ramp 实现

REAPER 7.82 的 Tempo Map 在 0 秒读回 120 BPM、2.000 秒（tick 3840）读回 stepped 60 BPM。Item 正好从该点开始，Target `contents`、250 ms、Snap off：41249 将 `D_STARTOFFS` 从0变为2.25秒、相位为4320/4800 PPQ；41250 推进到2.5秒和4800 PPQ loop endpoint（phase 0）。item bounds、notes、loop flag 和 selection不变。原厂实窗截图、fixture 与[ReaScript](../../scripts/reaper_parity/probe-item-nudge-midi-stepped-boundary.lua)见[阶跃边界记录](../verification/reaper-parity/item-nudge-midi-stepped-boundary.md)。App 回归改为覆盖同一 Tempo Point；MIDI source phase 现在跨 stepped 与 linear Tempo Map 都按 source-time 往返换算，ramp case 尚有6.985 PPQ长度量化误差。

App Item Nudge 定向回归19/19、AAADAW 桌面全测586/586、Core timebase16/16；Core + App all-target Clippy `-D warnings`、fmt 通过。最新桌面二进制 Build ID `7d8c6635e14449ba2e3240a1f51df92f38c59cde`、SHA-256 `89e74a265873cb0029de0214cbaa6e8f60a16e2c73b1cfa0ead46f912f54a556` 使用 REAPER 运行包中的 Xvfb 启动，实际主窗口 1280×800，截图与标准 startup fixture 像素一致；[桌面运行记录](../verification/reaper-parity/desktop-runtime.md)保存了截图和哈希。直接运行的失败由当前会话没有 DISPLAY/Wayland 引起；仓库 `build-and-smoke-aaadaw-desktop.sh` 会创建隔离 Xvfb 并选择可用 OpenGL 后端，已验证新二进制正常启动。source-loop 长度精度、其他 TempoCurve、Nudge Target/Set/Snap 和完整 Undo 语义仍未完成；`ITEM-NUDGE-SET-001`、P4 与唯一 PR #282 继续 `in_progress`。


### 2026-10-15 Looped MIDI absolute Set → Contents and desktop relaunch

Linux REAPER 7.82 / Default 7 的原厂 Nudge/Set 实窗验证了 looped MIDI 的绝对 Set → Contents。一个位于 2 秒、长 5 秒、source loop 为 9,600 PPQ 的 MIDI item，Snap off、目标 3 秒时保持 item bounds 和 selection，`D_STARTOFFS` 变为 4 秒、phase 为 7,680 PPQ。Snap on、目标 3.500 秒时，`minutes:seconds` 单位吸附到 4 秒，偏移变为 3 秒、phase 为 5,760 PPQ。前后窗口截图、两个 TSV 和 [ReaScript](../../scripts/reaper_parity/probe-item-nudge-midi-set-contents.lua)已保存到[验证记录](../verification/reaper-parity/item-nudge-midi-set-contents.md)。

AAADAW 现在只对 selected loop-source MIDI 支持绝对 Set Contents；Relative Set 在该目标下重置并隐藏。Snap 单位按 REAPER 的时间单位整秒吸附。所选 MIDI placements 通过一个 BatchTransaction 更新，并支持 Undo/Redo；本轮原厂与 App 回归都覆盖单个 item，多选行为仍需额外对照。App 定向回归覆盖 Snap off/on 两个原厂值、item 边界、selection 与 Undo/Redo；AAADAW 桌面完整测试 587/587 通过，定向测试 1/1；App all-target Clippy `-D warnings`、fmt 与 diff 检查通过。

新桌面构建 Build ID `dc0b012c46ad970d9c1c934e068833f18ff7a9a3`、SHA-256 `872f833897e4333b8dd039002ae6867fa188d35dce37cc061cbc24c0f616893a` 已由构建烟测脚本在捆绑 Xvfb :90 启动，实际主窗 1280×800；截图与标准 startup fixture 字节一致。该会话没有系统 Xvfb 或 DISPLAY，`scripts/reaper_parity/build-and-smoke-aaadaw-desktop.sh` 自动发现 REAPER runtime 的 Xvfb 并设 `WGPU_BACKEND=gl`，完整解决了新二进制的桌面启动验证路径。Set/Contents 实际窗口截图见对应验证记录。

该切片不等于完成 Nudge/Set 验收。Audio/mixed-media Set Contents、MIDI 非 loop source、变速 Tempo Map、多种 Snap unit、其他 target 和更广的 Undo 仍未对齐；`ITEM-NUDGE-SET-001`、P4 与唯一 PR #282 继续 `in_progress`。

### 2026-10-15 Audio absolute Set → Contents

Linux REAPER 7.82 / Default 7 原厂 probe 在 48 kHz、4 秒 WAV、item 位置 2 秒/长度 5 秒上记录了三个 Audio Set 结果：Loop source 开、Snap off、目标 3 秒得到 source offset 3 秒；Snap on、目标 3.5 秒吸附为 4 秒后得到 2 秒；Loop source 关、目标 3 秒得到 −1 秒。item bounds 和 selection 均保持不变。TSV、ReaScript 与三张原厂窗口截图见[Audio Set → Contents 验证](../verification/reaper-parity/item-nudge-audio-set-contents.md)。

AAADAW 现在按 item start 与绝对 target 的项目采样差换算到素材采样率；Loop source 对完整素材或有效 source section 环绕，非循环源保留 signed offset。选中的 Audio 与 loop-source MIDI 先整批校验，再作为一个 `BatchTransaction` 提交，未提供素材元数据时不会部分改动 MIDI。定向回归覆盖 96 kHz source/48 kHz project 换算、原厂 loop/snap/non-loop 数值、bounds、selection、Undo/Redo，混合 Audio/MIDI 同事务及缺元数据整批失败。另修正一个已有 bounded-section 播放测试夹具：schema 50 后 source offset 相对 section 起点，fixture 从旧绝对 offset 改为 section 内 0 帧。

Stable `cargo xtest --no-fail-fast` 在隔离 Xvfb 完整通过 1,142/1,142，`aaadaw` 与 `aaadaw-app` all-target Clippy `-D warnings`、fmt 检查通过。最新桌面二进制 SHA-256 `9011a9c4ab75512ee931b176bc80c77516d0a6745d17644715016aacdee722b3` 由仓库启动脚本在 Xvfb :90 显示真实 1280×800 主窗；Set/Contents/Snap-on 对话框截图来自同一二进制，截图 SHA-256 `0905f30e8a42f432135d487da0b3d290b8e9fdbdddb43d16d478aec9ccb60fe4`。该 AAADAW 功能截图没有选中 item，仅用于核对真实弹窗状态，不作为音频偏移结果证据。

Native bounded-section Set、MIDI 非 loop source、多种 Snap unit、其它 target 与更广的 Undo 语义仍需补齐；本切片不关闭 `ITEM-NUDGE-SET-001` 或 P4，唯一 PR #282 保持 `in_progress`。

### 2026-10-15 混合 Audio/MIDI 绝对 Set → Contents 原厂对照

Linux REAPER 7.82 / Default 7 在同一个选中 Audio/MIDI selection 上执行 Mode `Set`、Target `contents`、绝对目标 3 秒、Snap off：两个 item 均位于 2 秒且长度 5 秒，loop source 开启。4 秒 Audio 源由 offset 0.1 秒变为 3 秒；10-quarter-note MIDI source 由 `D_STARTOFFS=0` 变为4秒、相位7,680/9,600 PPQ。两个 item bounds、MIDI 两条 note 与 selection 不变。一个 Undo 同时恢复 Audio/MIDI offsets，再一次 Redo 同时重做；TSV 和四张原厂实窗截图见[混合 Set → Contents 验证](../verification/reaper-parity/item-nudge-mixed-set-contents.md)。

AAADAW 现有 Audio 与 loop-source MIDI Set action list 在同一次 `BatchTransaction` 提交；混合成功、单步 Undo/Redo 和 Audio metadata 缺失时整批拒绝均有 App 回归。原厂 probe 补齐了之前未测的混合 Set 边界；`ITEM-NUDGE-SET-001` 仍为 `in_progress`，因为 bounded Audio Set、MIDI non-loop source、其它 Snap unit/targets 与更广 Undo history 仍未完成。

### 2026-10-15 Non-loop MIDI absolute Set → Contents

Linux REAPER 7.82 / Default 7 原厂 probe 覆盖了目标早于与晚于 2 秒 item 起点的两种分支。目标 1 秒时 `D_STARTOFFS=1.0`，10-quarter-note source 和 notes 保持不变；目标 3 秒时 offset 清零、notes 向后移动 1,920 PPQ、source 长度从 10 增为 12 quarter notes。item 仍为 2–7 秒、Loop source 关闭且 selection 保留。Set 一次 Undo 恢复 offset/notes/source length，Redo 再应用。TSV、实际 REAPER 窗口和 [探针](../../scripts/reaper_parity/probe-item-nudge-midi-nonloop-set-contents.lua)见[验证记录](../verification/reaper-parity/item-nudge-midi-nonloop-set-contents.md)。

AAADAW 新增 `SetMidiItemContents` domain action：早于 item 起点时设置正 source offset；晚于起点时同量平移 notes、CC、Pitch Bend 并扩展 MIDI source length。事务保留 note IDs，item 边界/selection 不变。App 回归覆盖两种目标方向、三类事件、Undo/Redo 和源状态。Workspace `cargo xtest --no-fail-fast` 通过 1,143/1,143；workspace all-target Clippy `-D warnings`、`cargo fmt --all -- --check` 与 matrix 校验通过。

新桌面 Build ID `c1e3833d58fbf8c1c257f9db9c8b2f25111dc592`、SHA-256 `69cfa5bc5e08ab5eb613c1602675caae5fedc64f59d7599607ccf05ed2dffc36` 通过 bundled Xvfb build-and-smoke helper，渲染真实 1280×800 主窗，startup 截图与标准 fixture 像素一致；另有同一构建的 Set → Contents 3 秒实窗截图，截图没有 selected item，只用于核对窗口和控件。新 TSV/截图和哈希、当前代码/回归、open differences 已加入 [`ITEM-NUDGE-SET-001` 矩阵](parity-matrix.csv)。线性 Tempo ramp Set、含 non-loop MIDI 的混合选择、更多 Snap/target 和完整 Undo/UI parity 仍开放；`ITEM-NUDGE-SET-001`、P4 与唯一 PR #282 继续 `in_progress`。

### 2026-10-15 Non-loop MIDI Contents Nudge source boundary

Linux REAPER 7.82 / Default 7 原厂探针补齐非循环 MIDI Contents Nudge 穿过 source offset 零点的三种表示：起始 offset 100 ms 时，左移 250 ms 把 offset 增至 350 ms、事件与 10 QN source 不变；右移 250 ms 把 offset 夹到零，事件前移 288 PPQ、source 扩到 10.3 QN；从零左移 250 ms 时 offset 仍为零，事件后移 480 PPQ、source 缩到 9.5 QN。更完整的前后态、截图和探针见[非循环 MIDI Nudge 验证](../verification/reaper-parity/item-nudge-midi-nonloop-contents.md)。

AAADAW 的非 loop MIDI Contents Nudge 现在保留非负 source offset，并在需要时通过 signed source-event shift 重基准 notes、CC 与 pitch bend，扩展或缩短 MIDI source。操作仍以一个 `SetMidiItemContents` 事务提交，Undo/Redo 保留 note IDs 与 item selection。新增 app regression 对照原厂 100 ms 与零 offset 状态；Nudge 定向 24/24、AAADAW App 全套 591/591、Core/App all-target Clippy `-D warnings` 均通过。新桌面二进制由仓库 smoke helper 构建并在独立 Xvfb 实际运行，1280×800 主窗与 Nudge → Contents 250 ms 实窗均截图验证；该路径使用捆绑 REAPER runtime 的 Xvfb 和 OpenGL，适用于当前没有 DISPLAY 的会话。Build ID、SHA、截图哈希见[桌面运行记录](../verification/reaper-parity/desktop-runtime.md)。格式、diff 与 matrix 检查通过；变量 tempo、source 起始事件越界和更广 Undo 语义仍开放，`ITEM-NUDGE-SET-001`、P4 与唯一 PR #282 继续 `in_progress`。


### 2026-10-15 Non-loop MIDI event underflow clamp

REAPER 7.82/Default 7 probe with a non-loop MIDI item at zero source offset, first note at PPQ 100, and Contents Nudge left 250 ms shows that the first note is clamped to PPQ 0. The other note shifts by the same 100 PPQ; `D_STARTOFFS` becomes 0.197916667 seconds (380 PPQ) and the source shrinks by only 100 PPQ. The native state and reproduction setting are recorded in the [underflow fixture](../verification/reaper-parity/fixtures/item-nudge-midi-nonloop-underflow-native.tsv) and [probe](../../scripts/reaper_parity/probe-item-nudge-midi-nonloop.lua).

AAADAW now applies the event shift only up to the earliest note, CC, or pitch-bend event; any leftover leftward motion becomes a positive non-loop MIDI source offset. The source length shrinks by the applied shift only. The App regression verifies all three event types, selection, and Undo/Redo; AAADAW app suite 592/592, Core/App all-target Clippy `-D warnings`, rustfmt, and `git diff --check` pass. Latest Linux desktop Build ID `2f0140b07bd07138d8e4ebfc02d963883ce2fff6` (SHA-256 `f1fc9b7cdf783a2e9c808d7ee7f119fbd110f39fea33d65f41d24488aeb13cf8`) launched through the bundled-Xvfb smoke helper and rendered the 1280×800 main window; its screenshot matches the standard fixture. A separate session opened Item → Nudge/set items... from the visible menu; the current-build 528×121 Nudge/Contents window is screenshot-verified at 1 ms with no selected item. Native CC/pitch-bend-specific underflow values, variable-tempo and multi-item Nudge, and broader REAPER Undo behavior remain open; `ITEM-NUDGE-SET-001`, P4, and the single PR #282 remain `in_progress`.


### 2026-10-15 Non-loop MIDI event-type source clamp

An additional REAPER 7.82 probe made the CC the earliest source event: CC at PPQ 50, note at 100, and pitch bend at 300. A 250 ms left Contents Nudge clamps the CC at 0 and shifts the note and pitch bend by the same 50 PPQ. `D_STARTOFFS` becomes 0.223958333 seconds (430 PPQ) and the source shrinks by 50 PPQ. The native fixture and ReaScript inputs record both note-led and CC-led underflow cases.

The App regression now matches the CC-led reference, including the note and pitch-bend positions and one-step Undo/Redo. The minimum event position is computed across notes, controllers, and pitch bends. A pitch-bend-led minimum and variable-tempo/multi-item behavior remain open.

### 2026-10-15 MIDI source loop length property editor

AAADAW 的 MIDI Item Properties 增加 `Source length (PPQ)` 字段，使用已持久化的 MIDI source loop length；合法编辑作为 `SetMidiItemContents` project event 应用，并保留 notes、visible item length 与 selection，支持 Undo/Redo。Loop source 开启时，缩短到 note/CC/pitch-bend 事件范围内会拒绝，以免事件静默绕回。该属性与 loop flag 一样在 playback active 时拒绝编辑，并请求 playback graph refresh。应用回归覆盖加长、缩短越界拒绝、事件保留和 Undo/Redo。

AAADAW App 全量测试 593/593、all-target Clippy `-D warnings`、fmt 与 diff 检查通过。最新二进制 Build ID `cefd0b9deef9f55f3d07f233dca911297cb761d2`、SHA-256 `81f1b343d1d26e796819219b43d085880f7fda6060518f0270c8f7e9ead66e28` 经 bundled-Xvfb 启动烟测，真实主窗 1280×800；之后在实窗中把 source length 从 3840 改为 7680 PPQ 并 Apply，526×390 MIDI Item Properties 截图见[验证记录](../verification/reaper-parity/midi-item-properties-loop-source.md)。尚无 REAPER 7.82 原厂 source-length 控件/缩短规则的匹配 probe；上面的拒绝规则是当前内容保护约束，不作为原厂行为结论。旧 trimmed empty MIDI source 长度恢复也未解决。`ITEM-PROPERTIES-001`、`ITEM-NUDGE-SET-001`、P4 与唯一 PR #282 均保持 `in_progress`。

### 2026-10-15 Native MIDI source properties follow-up

REAPER 7.82 / Default 7 中，从 F2 `Media Item Properties` 的 `Take media source > Properties...` 打开的 `MIDI item properties` 对话框为 421×507；源长度显示为只读的 quarter-note 文本，没有编辑控件。Main action 40011 打开相同对话框。新 ReaScript、截图与实测状态保存在[原生 source properties 记录](../verification/reaper-parity/midi-item-properties-loop-source.md)及其 fixture；因此 AAADAW `Source length (PPQ)` 是额外编辑能力，不能作为原厂控件完全对齐的证据。`ITEM-PROPERTIES-001` 继续 `in_progress`。

### 2026-10-15 Looped MIDI Set → Contents across stepped tempo

Linux REAPER 7.82 / Default 7 的空白工程在 4.000 秒加入 60 BPM tempo step；选中一个 3.900–8.900 秒、source length 4,896 PPQ 的 looped MIDI item，在 `Nudge/Set Items` 设 Mode `Set`、Target `contents`、目标 4.150 秒、Snap off 后应用。REAPER 保持 item 边界、loop 与 note PPQ 不变，将 source offset 设为 2.300 秒、source phase 设为 4,416 PPQ，loop endpoint 为 9,312 PPQ，state-change count 只增加一次。前后状态、脚本与实际窗口截图记录在[变速 Set 原厂对照](../verification/reaper-parity/item-nudge-midi-variable-tempo-set.md)。

AAADAW 回归 `item_nudge_set_looped_midi_contents_matches_native_across_tempo_change` 对照上述 PPQ/source offset，检查 item、notes 与 selection 保持不变，并验证一次 Undo/Redo。Nightly Cranelift 的 focused nextest 通过 1/1（另 593 项按筛选跳过）。Cranelift 桌面二进制 SHA-256 `dcd9f1a0920e91581eda4a7d1aae661b89d29552b44dde8f7e422921d1046386` 在 bundled Xvfb :90 显示 1280×800 主窗；startup 帧与标准 fixture 逐字节一致（SHA-256 `ae7790daa7323931332582152cd05219af623ac5530147835ee56910ba2ebd9b`）。该构建未启用 `audio-device`，所以这项截图只验证窗口启动与渲染。`ITEM-NUDGE-SET-001` 仍 `in_progress`；线性 tempo ramp、混合 non-loop MIDI 选择、其它 Snap 单位、Target 与更广的 Undo/UI 对齐仍开放。


### 2026-10-15 Staggered Item Nudge Set edge parity and desktop run

Linux REAPER 7.82 / Default 7 native probes with loop-source Audio items at 2s and 6s show that absolute Set puts each selected item's corresponding boundary at the entered destination. Left Edge 4s makes both starts 4s and lengths 8s/12s while preserving source offsets. Left Trim 4s produces the same positions and lengths and advances both source offsets by 2s, including the item that started after the destination. End Position 14s makes both ends 14s; Right Trim 14s does the same without moving starts. End Position Relative Set interprets 14s as an absolute destination for the last edge at 16s, then moves all items by −2s. The native TSV, probe, and actual Left Trim result screenshot are archived under `docs/verification/reaper-parity`.

AAADAW Set edge handling now applies those per-item absolute boundaries; Relative Set computes one delta from the last selected end. Audio Left Trim source movement follows the measured nonnegative distance and source sample rate. The staggered Audio regression checks all four targets, source frames, and Undo/Redo. Nightly Cranelift App nextest passed 602/602 (the focused Item Nudge filter is 34/34); Stable App Clippy `-D warnings`, rustfmt, and `git diff --check` passed.

The rebuilt nightly Cranelift desktop executable, Build ID `027235a78569b58fbf6072409f0a74d65336d0c3`, SHA-256 `b6f8ce725ab437fab8f212d0d79e5709a4ff0176a8923243fd0e2414a7e319fa`, launched through the smoke helper on bundled Xvfb :90 with an actual 1280×800 main window. The startup image matches the baseline fixture byte-for-byte. The helper isolates XDG state, selects `WGPU_BACKEND=gl`, and resolves the missing system Xvfb / unset DISPLAY environment without changing application runtime code.

`ITEM-NUDGE-SET-001`, P4 and the single PR #282 remain `in_progress`. Native staggered MIDI/mixed-media edges, other Unit/Snap combinations, variable-tempo MIDI edge precision, broader Undo parity, and full REAPER UI parity remain open.


### 2026-10-15 Staggered loop-source MIDI Set edge parity

REAPER 7.82 / Default 7 with two 10-second loop-source MIDI items at 2s and 6s confirms that Left Trim, Left Edge, End Position, and Right Trim use the same shared absolute edge destinations and timeline geometry as Audio. MIDI note PPQ stays unchanged. Left Trim applies a signed source-phase displacement: at a 4s target, the first item source offset becomes 2s and the later item wraps backward by 2s to 8s in its 10-second source loop. A fresh-project Left Trim is one native Undo step; Undo restores both original items and Redo reapplies both positions and wrapped phases. The probe, full native state TSV, and actual Left Trim window capture are archived under `docs/verification/reaper-parity`.

AAADAW now carries the signed displacement through the MIDI tempo map and wraps a negative offset by that item's source-loop duration before converting back to PPQ. Audio keeps the separately measured absolute-distance Left Trim behavior. The new staggered MIDI regression covers all four edge targets, End Position Relative Set, note preservation, loop phase, and one-step Undo/Redo. Nightly Cranelift App nextest passed 603/603; Stable App Clippy `-D warnings`, rustfmt, parity-matrix parsing, and `git diff --check` passed.

The latest nightly Cranelift desktop binary (Build ID `e284d4328a874c8155f86697481144d8bf124b34`, SHA-256 `11483a8e032fb40d03167a70b7fb19c69c76390f499c0a7a7392ec6239abe03e`) launched through the smoke helper on the bundled Xvfb :90 and rendered the real 1280×800 main window. Its screenshot is pixel-identical to the baseline fixture. The smoke path still works without `DISPLAY` or system Xvfb, using `WGPU_BACKEND=gl` and a fresh XDG profile.

`ITEM-NUDGE-SET-001`, P4 and the single PR #282 remain `in_progress`. Native staggered mixed-media/non-loop MIDI edges, other Unit/Snap combinations, variable-tempo MIDI edge precision, broader Undo parity, and full REAPER UI parity remain open.


### 2026-10-15 Staggered non-loop MIDI Set → Left Trim parity

Linux REAPER 7.82 / Default 7 with two selected non-loop MIDI items at 2s and 6s, each 10s long with 20 QN sources, confirms Set → Left Trim at 4s rebases source data as well as item geometry. The first item ends at 4–12s with `D_STARTOFFS=1.5s`, source length 19 QN, and notes at 0/1,920 PPQ; the later item ends at 4–16s with zero source offset, a 24 QN source, and notes at 4,800/6,720 PPQ. One Undo restores both source states and Redo reapplies them. The [native TSV and after screenshot](../verification/reaper-parity/item-nudge-midi-set-edge-nonloop.md) record the actual window and state.

AAADAW now combines placement edits with `SetMidiItemContents` in one transaction. It adjusts take phase with the moved edge and rebases non-loop notes, CC, pitch bends, and source length at PPQ zero when required. The App regression covers both staggered directions, selection, and one-step Undo/Redo. The focused native-match test passed; the full AAADAW App suite passed 604/604. Stable App all-target Clippy `-D warnings`, rustfmt, diff checks, and parity-matrix validation passed.

The current stable desktop binary (ELF Build ID `ae5d78c679f41119b767d24ca48b015c01b0fe5b`, SHA-256 `22257c1a633ac6b3518cf68702b535733cbd0048d8db0856b91c796c62fab463`) ran through the repository desktop helper on bundled Xvfb :90. Its real 1280×800 main window was visible, and the [startup screenshot](../verification/reaper-parity/fixtures/aaadaw-nonloop-midi-set-edge-desktop-smoke.png) matches the baseline fixture byte-for-byte. The helper isolates XDG state and uses OpenGL, so it works in this session without `DISPLAY` or system Xvfb. Native fixture hashes are recorded in `fixtures/item-nudge.tsv`.

`ITEM-NUDGE-SET-001`, P4 and the single PR #282 remain `in_progress`. Native non-loop MIDI Left Trim is now covered; non-loop MIDI Left Edge/End Position/Right Trim, staggered mixed-media selection, other Unit/Snap cases, variable-tempo edge precision, broader Undo parity, and complete REAPER UI parity remain open.


### 2026-10-15 Staggered non-loop MIDI Set edge completion

REAPER 7.82 / Default 7 native probes now cover the remaining non-loop MIDI Set targets for staggered items at 2s and 6s. Left Edge to 4s preserves both 20 QN source lengths; End Position to 14s moves both starts to 4s and also preserves those lengths. Right Trim to 14s keeps the starts at 2s/6s, changes visible lengths to 12s/8s, and extends/truncates source lengths to 24/16 QN without moving MIDI events. Applying Right Trim after Left Trim retains the first item's 1.5s source offset and yields a 23 QN source. Native Undo/Redo restores and reapplies the Right Trim as one action. State captures and real REAPER window screenshots are linked from the [native comparison record](../verification/reaper-parity/item-nudge-midi-set-edge-nonloop.md).

AAADAW Set → Right Trim now updates a non-loop MIDI source length to source offset plus the new visible length, preserving all source events in the same transaction. The staggered regression covers Left Edge, End Position, direct Right Trim, and Right Trim after Left Trim, including source data, selection and Undo/Redo. The AAADAW App suite passed 605/605; App Clippy with `-D warnings`, rustfmt, `git diff --check`, and parity-matrix validation passed.

The rebuilt desktop binary (ELF Build ID `fe205612074f843fc93c028843ede638dcf45283`, SHA-256 `5f3c68aa984b9d47a7cfa08fca911a283b17fcfdda8b5b135307cbe712074636`) launched through the repository helper on bundled Xvfb :90 and rendered the actual 1280×800 main window. Its screenshot SHA-256 `ae7790daa7323931332582152cd05219af623ac5530147835ee56910ba2ebd9b` is byte-identical to the startup baseline.

`ITEM-NUDGE-SET-001`, P4 and the single PR #282 remain `in_progress`. Staggered mixed-media edge selection, other Unit/Snap combinations, variable-tempo MIDI edge precision, broader Undo parity, and complete REAPER UI parity remain open.


### 2026-10-15 Staggered mixed-media Set edge parity

Linux REAPER 7.82 / Default 7 native probes now cover a staggered selection with loop-source Audio at 2s and loop-source MIDI at 6s. Set → Left Trim to 4s moves both left edges to 4s; Audio source offset advances to 2s and the later MIDI item wraps backward to 8s in its 10s loop. Left Edge to 4s preserves both source offsets; End Position to 14s moves both ends to 14s; Right Trim to 14s keeps starts at 2s/6s and changes lengths to 12s/8s. Notes, CC, and pitch bend stay unchanged. Every Apply is one `Nudge items` undo step, and Undo/Redo restore both items and selection. The actual REAPER captures and full state log are linked from the [edge target comparison](../verification/reaper-parity/item-nudge-set-edge-targets.md).

The new mixed-media regression compares both item geometries and source phases for all four targets, preserves MIDI expression events, and verifies one-batch revision plus Undo/Redo. AAADAW App package nextest passed 606/606; Clippy with `-D warnings`, rustfmt, and diff checks passed. A full workspace `cargo xtest` attempt stopped before test execution while linking `aaadaw-engine`'s `audio_sends` target because the environment filesystem filled; the App package nextest completed after reclaiming generated test artifacts.

The current default-feature desktop binary (Build ID `2112663f480d734f50aca9cd6c1da2a37a985e96`, SHA-256 `7354fe8daf387d1aa65c6e6a8a39399a600fafd8ec6bdf6345cf864e01abb42b`) launched on bundled Xvfb :90 with a real 1280×800 main window. Its screenshot SHA-256 `ae7790daa7323931332582152cd05219af623ac5530147835ee56910ba2ebd9b` matches the startup baseline byte-for-byte.

`ITEM-NUDGE-SET-001`, P4 and the single PR #282 remain `in_progress`. Non-default Units and Snap combinations, mixed Relative Set, variable-tempo MIDI edge precision, playback state, wider Undo parity, and complete UI parity remain open.


### 2026-10-15 Mixed Audio/MIDI edge Nudge Snap and desktop launch

Linux REAPER 7.82 / Default 7 confirms Snap to unit for Nudge Left Trim, Left Edge, End Position, and Right Trim on a selected loop-source Audio item at 2 s plus loop-source MIDI item at 6 s. With `0.6 milliseconds`, the first item's start rounds to 2.001 s and one shared 1 ms delta is applied across the group. Left Trim advances both source offsets; Left Edge preserves them; End Position moves both items without resizing; Right Trim extends both lengths. Notes, CC, and pitch bend remain unchanged. A second phase-offset probe (`2.0004 s`, `0.8 ms`) confirms the Nudge snap anchor is still the first selected start: End Position and Right Trim apply 0.6 ms to the group instead of independently rounding each item's edge. Native TSVs and actual REAPER screenshots are linked from the [edge Snap verification](../verification/reaper-parity/item-nudge-edge-snap.md).

AAADAW now applies the same snapped group delta to all four Nudge edge targets and supports mixed Audio/MIDI selections. Audio Left Trim retains source-sample-rate conversion; MIDI placement/source phase goes through the project tempo map, and note, controller, pitch bend, loop phase, selection, transaction and Undo/Redo behavior are checked. The new mixed-target Snap regression passes with Snap both on and off. Focused nextest passed 1/1; the complete AAADAW App package passed 628/628 with zero skipped. App Clippy `-D warnings`, rustfmt, and `git diff --check` passed. Workspace-wide linking still cannot complete within this overlay's available space; the App package validation does complete.

The new desktop executable (Build ID `a3368be34af07e2a120c3f66440763b81052016a`, SHA-256 `3c2d09703677e0e63d86dd69726ffa210c08429e255321bf227519e30c46fd19`) launched through the repository smoke helper on bundled Xvfb `:90` and rendered the actual 1280×800 main window. Its screenshot matches the standard startup fixture byte-for-byte (SHA-256 `ae7790daa7323931332582152cd05219af623ac5530147835ee56910ba2ebd9b`); an isolated `:200` session also opened Item → Nudge/set items..., where `xwininfo` reported the real 528×121 dialog. The live dialog capture hash is `bc10e4f08dc3f21bfc5b9d3ec6233c3eb7180c5c5ff62c783438dcb516f5da7a`. This resolves the missing `DISPLAY`/system-Xvfb launch issue for the rebuilt binary without changing runtime code. `ITEM-NUDGE-SET-001`, P4, and the single PR #282 remain `in_progress`; other Units, variable-tempo MIDI edge precision, broader native Undo parity, and full REAPER UI parity remain open.

### 2026-10-15 Non-loop MIDI Nudge edge source behavior

Linux REAPER 7.82 / Default 7 native probes now cover non-loop MIDI Nudge Left Trim, Left Edge, End Position, and Right Trim in a mixed Audio/MIDI selection. At source offset zero, Left Trim physically removes source prefix only through the earliest MIDI event; events already at 0 PPQ cause the remaining trim to advance `D_STARTOFFS`. A positive source offset advances before the source is rebased; crossing source zero from 0.1 seconds retains a 1 PPQ offset and shifts note, CC, and pitch-bend data by 289 PPQ. Leftward expansion from zero extends the source and moves every event right. All changes use the native `Nudge items` Undo label. The complete measurements, source-start case, and actual REAPER captures are in the [non-loop MIDI Nudge record](../verification/reaper-parity/item-nudge-midi-nonloop-edge-nudge.md).

AAADAW now uses separate Nudge and Set MIDI edge transforms. Regression coverage checks all four targets, both source-zero cases, the positive-offset and cross-zero cases, event content, and one-step Undo/Redo. The full AAADAW binary test target passed 610/610; Stable App Clippy `-D warnings`, rustfmt, and `git diff --check` passed. The rebuilt desktop binary (Build ID `ef6c95f6ab1f03277a9ac6f4f8949cca1a5372b1`, SHA-256 `a163ffa157fba44113c4c890aceea8e16f39c917a2469798883bcace71b335c3`) launched on bundled Xvfb :93 with the actual 1280×800 window. The startup image matches the standard fixture; in a separate :200 session the visible Item menu opened a real 528×121 Nudge/Set dialog. Both screenshots and the Build ID are in the [desktop runtime record](../verification/reaper-parity/desktop-runtime.md).

`ITEM-NUDGE-SET-001`, P4, and the single PR #282 remain `in_progress`. Other Nudge Units and Snap rules, variable-tempo MIDI edge precision, broad Undo parity, and full interface parity remain open.

### 2026-10-15 Linear Tempo Map and MIDI edge Nudge parity

REAPER 7.82 / Default 7 native comparison confirmed that a Linear BPM ramp is linear over project time: 120→60 BPM ending at measure 3 reaches 5.333333 seconds. With the MIDI item at 3.900 seconds, a 250 ms Left Trim moves its source events by 298 PPQ, shortens the source to 5,462 PPQ, and retains a 0.875-PPQ item-edge phase (`D_STARTOFFS=0.000745908s`). The [native TSV, ReaScript, and before/configured/after screenshots](../verification/reaper-parity/item-nudge-midi-linear-tempo-edge.md) record the full state and hashes.

AAADAW `TempoCurve::Linear` now uses that project-time interpolation in both tick/sample directions. `sample_at_tick(7,680)` on the reference ramp returns exactly 256,000 samples at 48 kHz. MIDI edge Nudge applies the precise fractional tick delta to its event transform before storing integer PPQ, matching the native 298-PPQ notes/CC/pitch-bend shift and 5,462-PPQ source length. Linear Contents Set/Nudge now stores phase 4,538 / 4,985 PPQ against the native 4,537.985 / 4,984.900 values; its calculated source offset is within 17 microseconds of native. App tests passed 611/611 and the complete Core test package passed under nightly Cranelift with test debug information disabled. Stable Core/App all-target Clippy with `-D warnings`, rustfmt, and diff checks passed.

The rebuilt desktop binary without the optional `audio-device` feature (Build ID `5ea21045845e93d7a119d6d972dbcf8a793a7f51`, SHA-256 `e0898351ad39f07455a86c7802dbb9914edf89b9e0c2ed26182404f075488cd0`) launched on bundled Xvfb :93 with its actual 1280×800 main window. Its startup screenshot matches the standard fixture byte-for-byte; the current build and [capture](../verification/reaper-parity/fixtures/aaadaw-linear-tempo-nudge-desktop-smoke.png) are in the [runtime record](../verification/reaper-parity/desktop-runtime.md). The image verifies startup/rendering; the feature behavior is verified by the native probe and App regression.

`ITEM-NUDGE-SET-001`, P4 and the single PR #282 remain `in_progress`. The integer MIDI model still cannot preserve the native sub-PPQ source loop length/phase remainder, and hidden non-loop source-tail event editing, other Units/Snap combinations, broader Undo behavior, and full interface parity remain open.

### 2026-10-15 Nudge minutes:seconds and sample Snap parity

REAPER 7.82 / Default 7's Nudge unit menu was compared directly: it offers `milliseconds`, `minutes:seconds`, `grid units`, `measures.beats`, `samples`, `frames`, `pixels`, `item lengths`, `item selections`, and `notes`. AAADAW previously offered `seconds`; its Position Nudge now exposes the native `minutes:seconds` label and parses project-time values. A native Audio item at 1.270s nudged by 250ms reaches 1.520s with Snap off and 2.000s with Snap on. At 44.1kHz, a one-sample Nudge reaches 1.270022676s, unchanged by Snap. Native values and screenshots are in [the Unit/Snap comparison](../verification/reaper-parity/item-nudge-units.md).

AAADAW App regression checks time and sample Nudge with Snap both ways, left/right destination rounding, preserved length/source offset/selection, and one-step Undo/Redo. The complete App suite passed 612/612 under nightly Cranelift; Stable Core/App Clippy `-D warnings`, rustfmt and diff checks passed. The rebuilt desktop binary (Build ID `9a9e51fce6f05b2a905e621d87e142e887e2cdac`, SHA-256 `209e9cd76eada5abcb7bbcbfbe4ac963efb25a98e315897388b16572d361b955`) rendered its actual 1280×800 window on bundled Xvfb :93; its startup capture matches the baseline pixel-for-pixel. A persistent Xvfb :413 session opened the actual 528×121 Nudge/Set dialog, showed the Unit dropdown and selected `minutes:seconds`; current-build screenshots and hashes are in the [runtime record](../verification/reaper-parity/desktop-runtime.md).

`ITEM-NUDGE-SET-001`, P4 and the single PR #282 remain `in_progress`. REAPER's measures.beats, frames, pixels, item lengths, item selections, and notes are still missing, along with other unverified Snap combinations, variable-tempo MIDI edge precision, wider Undo parity, and full interface parity.

### 2026-10-15 Grid-unit Nudge

Native REAPER 7.82 measurements establish that one `grid unit` follows the current Arrangement grid: with a quarter-note grid at 120 BPM, the selected item at 1.270s moves to 1.770s without Snap, to 2.000s with Snap, and to 1.000s for a left Nudge with Snap. AAADAW now converts the selected item's musical target through the project Tempo Map and current PPQ grid, then snaps that target before applying one shared movement delta. The focused regression and full App suite pass 613/613 under nightly Cranelift.

The rebuilt desktop binary (Build ID `cbe01acbde2941ccfec7f4aeece517977c368584`) launched through the repository's bundled-Xvfb smoke helper; the actual 1280×800 startup frame matches the baseline. I opened Item → Nudge/set items... in the same executable and confirmed `grid units` and `Snap to grid` in the live dialog. Native measurements, screenshots and desktop captures are in [the unit verification record](../verification/reaper-parity/item-nudge-units.md).

The native probe also measured `measures.beats` (`1.0` moves one bar; `0.1` moves one beat at the tested 4/4 position), but AAADAW does not implement it yet. `frames`, `pixels`, `item lengths`, `item selections`, `notes`, wider variable-tempo/Snap coverage, and the mismatch between native Undo state-counter readings remain open. `ITEM-NUDGE-SET-001`, P4, and the single PR #282 remain `in_progress`.

### 2026-10-15 Measures.Beats Position Nudge

Linux REAPER 7.82 / Default 7 native measurements show that `measures.beats` value `1.0` advances one 4/4 bar (2s at 120 BPM), `0.1` advances one beat, and Snap to bar rounds a 1.270s start plus one bar to 4.000s. AAADAW Position Nudge now parses measure/beat counts using the active meter at the first selected item, maps the destination through the Tempo Map, and moves the selection by one shared project-time delta. Snap rounds to the nearest measure boundary. App regression covers these native 4/4 values, static 3/4, preserved item geometry and selection, and one-step Undo/Redo.

Nightly Cranelift nextest passed the complete AAADAW package suite, 635/635 with zero skipped. Stable Core/App all-target Clippy with `-D warnings`, rustfmt, `git diff --check`, and the 100-row / 100-unique-ID parity-matrix parse passed. Cranelift emitted its known iced/wgpu `TimelinePipeline: MaybeSync` recursion-depth warning; Stable Clippy emitted no warnings. The updated desktop executable (Build ID `8e2bec90638e81a4d6a47ca85f703bf9e676c229`, SHA-256 `911806153bce6ec91006fc0f1bea1a44425d17384ee8b4f3932f40eb94887bb3`) launched on bundled Xvfb with the standard 1280×800 startup frame. The real Nudge/Set dialog showed `measures.beats`, `1.0`, and `Snap to bar`; scrolling the Unit menu exposed `samples`.

`ITEM-NUDGE-SET-001`, P4, and the single PR #282 remain `in_progress`. Variable-meter transitions, Measures.Beats on other targets, `frames` (implemented and verified in the following entry), `pixels`, `item lengths`, `item selections`, `notes`, PickList keyboard navigation, native Undo-counter parity, and broader Nudge/Set/UI coverage remain open.

### 2026-10-15 Frames Position Nudge

REAPER 7.82 / Default 7 native measurements confirm that at 30 fps a one-frame Nudge from 1.270s reaches 1.303333333s with Snap off and 1.300000000s with Snap on. One-frame Snap left reaches 1.233333333s; a half-frame Nudge reaches 1.286666667s without Snap and rounds to 1.300000000s with Snap. The item length, source offset and selection stay unchanged. Values, native undo labels and results are in [the Frames probe TSV](../verification/reaper-parity/fixtures/item-nudge-frames-native.tsv), with actual REAPER menu, setup, and after-action screenshots in the same fixture directory.

AAADAW now shows `frames` after `samples`, uses the project frame-rate ratio for Nudge conversion, and snaps destinations to the nearest frame boundary. Regression covers the native 30 fps cases, half-frame amounts, left/right Snap, geometry, selection and Undo/Redo; a separate 29.97 fps case checks non-integer samples per frame. The actual rebuilt dialog shows `frames`, default `1`, and `Snap to unit`; the scrolled menu capture shows its placement after `samples` ([selected control](../verification/reaper-parity/fixtures/aaadaw-frames-live-selected.png), [Unit menu](../verification/reaper-parity/fixtures/aaadaw-frames-live-unit-menu-scrolled.png)).

Nightly Cranelift passed the complete AAADAW package suite, 636/636 with zero skipped. Stable Core/App all-target Clippy with `-D warnings`, rustfmt and `git diff --check` passed. The current desktop binary (Build ID `d6004a0e999ea345f302e2bc780614ba0deb6719`, SHA-256 `e15c4af3705c28e390716ed3de02efc563fda5c8425ee142eb9931b544198d3b`) launched on bundled Xvfb; its startup frame matched the baseline and the real 528×121 Nudge/Set controls and scrolled Unit menu were captured.

`ITEM-NUDGE-SET-001`, P4, and the single PR #282 remain `in_progress`. Native Frames comparisons currently cover Position Nudge at 30 fps; fractional-rate REAPER behavior, Frames on other targets, remaining `pixels`, `item lengths`, `item selections`, and `notes` units, variable-meter transitions, keyboard selection in the Unit list, native Undo-counter consistency, and wider Nudge/Set/UI parity remain open.

### 2026-10-15 Pixels Nudge and Set units

Implementation and native evidence were committed as `0a93b46` on the existing `feat/reaper-parity` branch.

REAPER 7.82 / Default 7 confirms that Position Nudge `pixels` follows horizontal Arrangement zoom. From 1.270s, 1 pixel without Snap reaches 1.278915305s in a 0–6s view and 1.287830609s in a 0–12s view. With Snap on, Nudge floors the destination to an integer pixel; a 0.5px Nudge and a snapped left move were also recorded. The native Set menu contains `minutes:seconds`, `measures.beats`, `samples`, `frames`, and `pixels`. Set Pixels is an absolute screen coordinate: target 200.5 reaches 1.787518574s with Snap off and rounds to pixel 201 / 1.791976226s with Snap on. Set Frames reaches 2.000s at frame 60 and rounds 60.5 to frame 61 at 2.033333333s. Set Measures.Beats reaches 4.125s at `3.1.25` and rounds `3.1.75` to 4.500s with a quarter-note grid. Native values and actual REAPER captures are in [the unit comparison](../verification/reaper-parity/item-nudge-units.md) and fixture manifest.

AAADAW maps Pixels Nudge through the current Arrangement origin and zoom scale, applies the native floor behavior for Nudge Snap and nearest-pixel rounding for Set Snap, and exposes Pixels, Frames, and Measures.Beats in Set. Regressions preserve geometry, selection and one-step Undo/Redo. The 50 focused `item_nudge_*` tests pass under Stable and nightly Cranelift; Cranelift emits the existing iced/wgpu `TimelinePipeline: MaybeSync` recursion-depth warning. The full Stable `aaadaw --no-default-features --bin` suite passes 618/618, all-target Clippy with `-D warnings`, rustfmt, and `git diff --check` pass. The rebuilt desktop binary (Build ID `9cc05baf398e22839d3f987c51c4e92a1b1fddc4`, SHA-256 `cf04765bef4a9e9751274c99132aa1bc2bfd115de823810e77a9a729569ac848`) rendered its actual 1280×800 main window on bundled Xvfb; its startup capture is byte-identical to the baseline. I opened Item → Nudge/set items... in the same executable, verified the 528×121 dialog, and captured live Nudge Pixels and Set Pixels controls and their Unit menus. The empty-project capture verifies control rendering/defaults; the native probe and App tests verify behavior.

`ITEM-NUDGE-SET-001`, P4, and the single PR #282 remain `in_progress`. Native pixel behavior was measured for Position; pixel Nudge/Set on other targets, other tempo/meter combinations, fractional-rate REAPER Frames, native Undo-counter parity, PickList keyboard navigation, and the native `item lengths`, `item selections`, and `notes` units remain open. No full-parity claim is made.

### 2026-10-15 Item Length, Item Selection, and Notes Position Nudge

REAPER 7.82 / Default 7 native probe confirms `item lengths` uses the first selected item's duration as one shared movement: a 0.500 s Audio A moves alone by 0.500 s, a 1.000 s Audio B moves by 1.000 s, and selecting both moves each by the first item's 0.500 s. `item selections` uses the envelope of selected item bounds: A/B at 1–5 s gives a 4.000 s step, while a single 8 s MIDI item gives an 8 s step. The 2–5 s time selection does not determine that step. `notes` is a musical value independent of media type and MIDI note lengths: at 120 BPM, 1/4 moves Audio and MIDI items by 0.500 s and 1/2 moves Audio by 1.000 s. The REAPER unit menus, complete before/after item state, and screenshots are recorded in the [relative-unit verification](../verification/reaper-parity/item-nudge-relative-units.md) and fixture manifest.

AAADAW Position Nudge now offers all three units. Item-length and item-selection movement uses one shared delta; the Notes selector exposes REAPER's 13 native note values and maps note lengths through project tempo. Regressions check unequal Audio lengths, mixed Audio/MIDI movement, item-selection bounds, quarter and triplet values, a 120→60 BPM tempo point, and undo/redo. Focused `item_nudge_*` tests pass 53/53 on Stable and nightly Cranelift; the full Stable `aaadaw --no-default-features --bin` suite passes 621/621. All-target Clippy with `-D warnings`, rustfmt, and `git diff --check` pass. Nightly Cranelift reports the existing iced/wgpu `TimelinePipeline: MaybeSync` recursion-depth warning.

Desktop Build ID `fb171ea46153cd1ad1d98de23d6395c41a02ff95` (SHA-256 `6066a08f870478bc508a1c4dc7be0b77787bfbe0b5c4ca3454c9139d9383bd95`) passed bundled-Xvfb smoke with the real 1280×800 main window; startup screenshot SHA-256 `ae7790daa7323931332582152cd05219af623ac5530147835ee56910ba2ebd9b` matches the baseline. The live 528×121 Nudge/Set dialog shows Notes, default 1/4, and amount 1; the scrolled Unit and note-value menus are captured with hashes in the [fixture manifest](../verification/reaper-parity/fixtures/aaadaw-desktop-runtime.tsv). `ITEM-NUDGE-SET-001`, P4, and the single PR #282 remain `in_progress`; Snap behavior for these units, other Nudge targets, variable-tempo native Notes confirmation, PickList keyboard navigation, wider undo parity, and the full interface remain open.

### 2026-10-15 Relative Nudge Unit Snap

REAPER 7.82 / Default 7 with Snap on confirms distinct Position behavior for the three new units. `item lengths` moves Audio A from 1.270 to 2.000 seconds after its 0.500-second step, snapping the 1.770-second destination to the Arrangement grid. `item selections` uses the 1.270–5.100-second selected-item envelope (3.830 seconds), then snaps its 5.100-second destination to 5.000 seconds and applies the shared 3.730-second delta to Audio B, placing it at 7.830 seconds. Notes `1/4T` targets 1.603333 seconds and snaps to 1.666667 seconds on the selected triplet-note grid, rather than the 1.500-second quarter-note Arrangement grid. The Nudge actions did not change the project state-change count. Values, script, and actual REAPER screenshots are recorded in the [relative-unit Snap verification](../verification/reaper-parity/item-nudge-relative-units.md).

The App regression now locks the quarter-triplet Snap rule and verifies Undo/Redo. All 54 focused `item_nudge_*` tests pass under Stable and nightly Cranelift, and the full Stable App suite passes 622/622; Cranelift emits the existing iced/wgpu `TimelinePipeline: MaybeSync` recursion-depth warning. Snap for other Units and Targets, variable-tempo native Notes behavior, PickList keyboard navigation, broader Undo parity, and full UI parity remain open. `ITEM-NUDGE-SET-001`, P4, and PR #282 remain `in_progress`.

### 2026-10-15 Edit Cursor Notes Nudge

REAPER 7.82 / Default 7 confirms Target `edit cursor` accepts Notes `1/4T` with no selected media: from 1.270 seconds Snap off reaches 1.603333 seconds, and Snap on reaches 1.666667 seconds on the triplet grid. Audio/MIDI positions and the project state-change count remain unchanged. The actual dialog capture, before/after values, and ReaScript are in the [Edit Cursor verification](../verification/reaper-parity/item-nudge-edit-cursor.md).


### 2026-10-15 Nudge/Set Selector Keyboard Navigation

Linux REAPER 7.82 / Default 7's actual Nudge/Set dialog confirms that opening Unit or Target and pressing Arrow Down then Enter commits the next choice (`milliseconds` → `minutes:seconds`; `position` → `left trim`). Escape dismisses an open list and leaves the dialog open. The 528×121 native captures and hashes are recorded in [the keyboard fixture](../verification/reaper-parity/fixtures/item-nudge-picker-keyboard.tsv).

AAADAW's Mode, Target, Nudge Unit, Set Unit, and Note Value selectors now support the same Arrow Down/Enter selection path and show a dropdown arrow. Set exposes only its five supported units. In the real 1280×800 Xvfb window, keyboard input changed Mode, Target, both units, and Note Value; Escape with a list open dismissed it first. Regressions lock all five option sets and route captured Escape to the selector before window-close handling. Focused Nudge tests pass 57/57, the full Stable App suite passes 625/625, workspace all-target Clippy with `-D warnings`, rustfmt, and diff checks pass. Desktop binary SHA-256 `34fb01f29fb29602125f8934063a55967a21b77c829fcb86afee2125b463eb63` passed the bundled-Xvfb smoke; the 1280×800 startup capture matches the baseline. See the [comparison record](../verification/reaper-parity/item-nudge-picker-keyboard.md) and [screenshot manifest](../verification/reaper-parity/fixtures/item-nudge-picker-keyboard.tsv).

Keyboard-only opening, Tab order, repeated-key timing, type-to-filter behavior, exact popup geometry/colors, other Unit/Target Snap cases, variable-tempo Notes, broader Undo parity, and complete UI parity remain open. `ITEM-NUDGE-SET-001`, P4, and the single PR #282 remain `in_progress`.

### 2026-10-15 Repeat Transport Block Planner

P6 / `TRANSPORT-LOOP-001` now has a real-time-safe `TransportLoopRange` and `advance_repeating_block` primitive. A callback can be represented as preallocated contiguous sample segments across the Loop end, including multiple wraps in one callback; each wrap advances the MIDI chase generation. Invalid/empty ranges are rejected, and insufficient segment capacity or sample-position overflow leaves transport state unchanged. The Stable `aaadaw-engine` suite passes 196/196 under bundled Xvfb; strict engine Clippy, formatting, diff checks, and the callback-thread zero-allocation probe pass.

`MidiEventPlan::events_for_segments` now schedules callback-relative MIDI events over those transport segments. At a backward loop wrap it releases active sustain, sends note-offs for notes sounding immediately before the edge, then chases controllers, pitch bends, and held notes at the destination; a short output buffer leaves the caller's output unchanged. The focused MIDI schedule suite passes 19/19, the full Stable engine suite passes under bundled Xvfb, and Stable engine all-target Clippy, formatting, and diff checks pass.

`AudioRenderGraph` now renders the planned transport segments inside one callback, so CLAP instrument routes and external MIDI output receive loop-boundary releases/chase at callback-relative offsets. A short MIDI output buffer is preflighted across every segment before PCM or transport changes; timeline AudioItem streams that would need a backward refill are likewise rejected before queue consumption. The realtime allocation probe now exercises repeated graph rendering.

### 2026-10-15 Loop-tagged file feeder bridge

Added a bounded stereo file feeder for the exact case where a loop-source AudioItem's full timeline range equals the transport loop, its bounded source section has the same frame count, its source offset is zero, and the decoded rate equals the project output rate. The worker repeats project-sample tags over that range; the render graph accepts the matching backward cursor reset and updates the shared feeder position at each wrap. This keeps decode and queue backpressure on the media thread; the callback still only reads prequeued PCM.

The file-backed media integration renders three passes through two wraps with sample-accurate left/right assertions and no underruns. The full `aaadaw-media` suite passes 39/39; engine render graph passes 37/37; the callback allocation probe passes; strict media/engine Clippy, formatting, and diff checks pass. The media worker is not yet wired to the application project loop state; mismatched ranges, ordinary/mono/audio-item feeders, resampling-rate conversion, seek/restart while the worker runs, isolated CLAP helpers, Repeat UI/actions, and Linux REAPER playback comparison remain incomplete. `TRANSPORT-LOOP-001` and `PROJECT-LOOP-001` remain `in_progress`.

### 2026-10-15 Repeat UI and application playback bridge

The application playback builder now accepts an optional `TransportLoopRange`; the existing no-loop API remains a delegating compatibility path. It resolves project Loop ticks through the project's tempo map, starts supported bounded stereo loop-source feeders at the correct in-range sample, and installs the range on the render graph. The Repeat button and Action List `Toggle repeat` share state, the action uses REAPER 7.82 Default 7's `R` binding, and changing Repeat or Loop points during playback prepares a replacement graph. Failed reconfiguration restores the previous Repeat value and pauses playback.

The current support boundary remains strict: an overlapping looped PCM item must be stereo, loop-source enabled, exactly match the timeline Loop range and bounded source section, start at source offset zero, and decode at the project output rate. Other overlapping audio items fail preparation before callback use; ordinary feeders that need consumed PCM at a backward seek and isolated CLAP helper scheduling remain unsupported. Repeat preference persistence and full visual parity remain open; native playback equivalence for the supported PCM case has not been compared.

Linux REAPER 7.82 / Default 7 with Dummy Audio, Loop `[1,2]`, and Repeat enabled keeps playback at its chosen start: after roughly 0.325 seconds, starts at 0, 1.5, and 3 seconds were at 0.325, 1.813, and 3.348 seconds. Starting after Loop therefore does not jump to its beginning on Play. The [native TSV](../verification/reaper-parity/fixtures/loop-repeat-start-native.tsv) and [ReaScript probe](../../scripts/reaper_parity/probe-loop-repeat-start.lua) preserve setup and measurements. The new app-library regression starts beyond the range and confirms transport advances linearly with no events from the earlier MIDI item.

Verification: `aaadaw-app`'s audio playback suite passes 20/20, including embedded stereo PCM assertions across multiple wraps with no underruns, MIDI-only note-on/off scheduling across an exact callback-edge wrap, and linear transport from a start beyond Loop. The two focused desktop App tests for the `R` action/state and tempo-map tick conversion pass 2/2. Audio-device strict Clippy (`-D warnings`) passes for engine, media, app-library, and desktop App; the no-audio desktop all-target check, rustfmt, and diff checks pass. The actual audio-device desktop binary (Build ID `2c08097b82d8cac86ab5de2965d2eef4d28a5a7c`, SHA-256 `ddf911badf00f1661ee337212bed8df1d255e8774741d0f921c7c179e8f86ee4`) rendered a 1280×800 main window on bundled Xvfb. The off-state and clicked on-state captures are [Repeat off](../verification/reaper-parity/fixtures/aaadaw-repeat-desktop-smoke.png) (SHA-256 `a8cbbc644d8c6ebc5605f5e9edb9d914cbd0d614cecc205693a07c38ce31fbcc) and [Repeat on](../verification/reaper-parity/fixtures/aaadaw-repeat-enabled-desktop-smoke.png) (SHA-256 `c353f4f9f6a74cc74ff60ce7e9cf745020d3e3298bc79d254fcc22c803702f04`); the latter visibly highlights the button and displays “Repeat enabled”.

`TRANSPORT-LOOP-001` and `PROJECT-LOOP-001` remain `in_progress`; the screenshot verifies AAADAW's own control state, not REAPER equivalence.

### 2026-10-15 Repeat playback for normal trimmed stereo items

Extended the transport-loop feeder to cover a normal stereo AudioItem whose timeline exactly matches Loop and whose nonnegative source offset selects an equally long source segment. This path uses the trimmed segment as the repeated section while retaining the bounded-section checks for explicit loop-source items; a disabled loop source, partial source-section metadata, negative offset, mismatched range, overflow, or output-rate conversion is still rejected before realtime rendering. The app regression renders from inside the segment through two loop wraps and checks both channels sample by sample with zero underruns.

The full app audio-playback suite passes 21/21 and `aaadaw-media` passes 39/39. The test environment lacked GLib development metadata, so the Rust test linked against the image's installed GLib runtime libraries through temporary `/tmp` pkg-config files; no repository dependency or system configuration was changed. `TRANSPORT-LOOP-001` and `PROJECT-LOOP-001` remain `in_progress`: overlapping items that do not meet the exact-range stereo contract, sample-rate conversion, ordinary feeder rewind/refill, isolated CLAP scheduling, native playback comparison, and remaining UI parity are still open.

### 2026-10-15 Linear audio playback after Loop

The earlier native REAPER 7.82 probe established that starting after Loop keeps the playhead there and advances linearly. App playback now applies that same rule to audio: if the start sample is beyond the Loop end, an overlapping item uses its ordinary seek feeder instead of being incorrectly forced into the exact-loop feeder. The regression starts at sample 35 in a stereo item spanning samples 24–40 while Loop is `[16,32)`, verifies source frames 19–23 without underruns, and confirms the transport reaches sample 40 without wrapping.

The full `aaadaw-app` package passes 98/98 tests; `audio_playback` passes 22/22; all-target strict Clippy, formatting, and diff checks pass. Repeat remains `in_progress`: items that overlap a reachable Loop without matching its full range, sample-rate conversion, ordinary feeder rewind/refill, isolated CLAP scheduling, preference persistence, and native audio playback comparison remain open.

### 2026-10-15 Transport Repeat ignores item Loop source mode

Separated transport Repeat from the per-item Loop source toggle for the supported normal trimmed stereo path. A transport loop now repeats the ordinary source frames selected by a nonnegative trim even when that AudioItem's Loop source option is disabled; explicit bounded source sections still require their own Loop source mode and the existing zero-offset/equal-length contract. A two-wrap app regression verifies sample-accurate stereo output and zero underruns with Loop source disabled.

The full `aaadaw-app` package remains 98/98, `audio_playback` 22/22, and `aaadaw-media` 39/39; strict media/app Clippy, rustfmt, and diff checks pass. This still covers only exact-range trimmed items at equal source/output rates; partial Loop overlap, rate conversion, refill, isolated CLAP playback, Repeat persistence, and native audio playback comparison remain open.

### 2026-10-15 Repeat a source segment from an item containing Loop

Extended the normal trimmed stereo feeder so the AudioItem may start before and end after the complete transport Loop. At an in-range play position, the feeder computes the source frame corresponding to Loop start, prepares exactly the Loop-length segment, then tags and repeats PCM over the project loop. This works with the item's Loop source option disabled. Items that overlap only part of the Loop remain rejected before rendering, and explicit bounded source loops retain their exact-range contract.

`transport_loop_preparation_repeats_the_source_segment_of_a_containing_item` renders from inside a `[16,32)` Loop contained by an item on `[8,40)`, checks both stereo channels through two wraps without underruns, and verifies preparation rejects both a cursor before Loop with that item already active and a second item with only partial Loop overlap. The full `aaadaw-app` package passes 99/99; `audio_playback` 23/23; `aaadaw-media` 39/39; strict media/app Clippy, rustfmt, and diff checks pass. Unequal-rate conversion, negative source mapping, partial-Loop items, pre-loop linear prefix plus loop handoff, feeder refill, isolated CLAP routes, Repeat persistence and native audio-loop comparison remain open.

### 2026-10-15 Mono source through Repeat playback

The existing stereo feed path also decodes mono sources into centered stereo output. Added an app regression for a mono item with item Loop source disabled, starting inside Loop and rendering through two wraps; both output channels match the expected centered PCM frame by frame with zero underruns.

The full `aaadaw-app` package passes 100/100, `audio_playback` 24/24, and `aaadaw-media` 39/39; strict media/app Clippy, rustfmt, and diff checks pass. Mono input is verified at the same source/output rate; unequal-rate conversion, partial-Loop items, pre-loop linear prefix handoff, refill, isolated CLAP routes, Repeat persistence and native audio-loop comparison remain open.

### 2026-10-15 Transport Repeat for partial-overlap audio items

Transport-loop feeders now derive the repeated source and project-time segment from the intersection of each normal trimmed AudioItem with the transport Loop. The render graph receives the same intersection boundary, so items ending before Loop end or beginning after Loop start rewind only across their own active segment. App preparation also keeps an overlapping item feeder alive after the current playhead passes that item, allowing it to play on the next transport wrap. Exact-range bounded source loops retain their prior whole-Loop contract; a pre-Loop active item still fails preparation until linear-prefix handoff is implemented.

`transport_loop_preparation_repeats_items_that_partially_overlap_the_loop` renders 44 frames from a `[16,32)` Loop over items `[8,24)`, `[24,32)`, and `[17,19)`. It checks source-offset mapping and PCM mixing through repeated wraps, including an item already past the playhead at start, with zero underruns. App/media nextest passes 136/136 (`aaadaw-app` 97/97 under `--no-default-features`, including `audio_playback` 25/25; `aaadaw-media` 39/39); affected-package strict Clippy, rustfmt, and diff checks pass. The workspace `cargo xtest` alias attempted to build all workspace test binaries but exhausted the local overlay disk during lld linking; the affected packages were run directly under nextest. `PROJECT-LOOP-001` remains `in_progress`: sample-rate conversion, pre-Loop linear-prefix handoff, refill, isolated CLAP routes, Repeat persistence, and native audio-loop equivalence remain open.

### 2026-10-15 Rate-aligned source resampling in transport Repeat

Transport-loop feeders now map normal trimmed source offsets and overlap durations from project samples to media frames using the decoded source rate. Unequal source/project rates are accepted when both boundaries map exactly to integer source frames and the resampled duration exactly matches the project Loop overlap. Bounded loop-source sections keep their whole-Loop item contract and also require an exact converted duration. Non-integral source boundaries fail worker preparation before the render graph starts. For differing rates, the media worker feeds enough complete loop cycles to warm the sinc filter's left history before publishing PCM, so each repeated pass uses periodic samples across its boundary.

`transport_loop_preparation_resamples_rate_aligned_source_segments` covers a trimmed item whose source offset and Loop start require a 24 kHz-to-48 kHz mapping, a bounded source section at the same rate ratio, three periodic passes without underruns, and rejection of a 15-project-sample overlap that would require a fractional source frame. App/media nextest passes 137/137 (`aaadaw-app` 98/98 under `--no-default-features`, including `audio_playback` 26/26; `aaadaw-media` 39/39); `cargo test -p aaadaw-app --no-default-features` passes 98/98, strict Clippy, rustfmt, and diff checks pass. The full workspace `cargo xtest` remains blocked by the overlay disk exhaustion recorded above; affected packages and the changed app package were verified directly. `PROJECT-LOOP-001` remains `in_progress`: fractional frame-boundary conversions, mid-stream decoder rate changes, pre-Loop active-item handoff, feeder refill, isolated CLAP routes, Repeat persistence, and native audio-loop equivalence remain open.

### 2026-10-15 Linear pre-Loop audio into transport Repeat

Normal trimmed audio items that begin before the active Loop now feed linearly from the playhead (or the later item start) through the item's Loop intersection, then repeat only that intersection. The media worker maps the prefix and repeated section to exact source-frame boundaries and preserves the prefix's project-sample tags until Loop start. A containing-item regression starts at sample 12 in an item spanning `[8,40)` with Loop `[16,32)`, checks the four-frame source prefix plus two full stereo Loop passes, and reports zero underruns. Added start-time edges verify that starting at sample 4 before an exact-range [16,32) item stays silent until Loop starts, and that starting at sample 4 before the containing item stays silent through sample 8, plays its linear source prefix, then hands off to repeated Loop audio; both render without underruns.

For differing rates, pre-Loop handoff now decodes the prefix and Loop section once in source order, streaming prefix PCM as it arrives while retaining only the bounded Loop section. The prefix resampler uses Loop-start right context, and the repeated resampler is seeded from cyclic samples at the Loop tail. This makes the first and later repeated passes identical, including when the Loop is shorter than the filter history. Only the decoded Loop source segment is buffered, capped at 1 MiB; larger rate-converted segments still fail worker startup with UnsupportedLoopResamplingPrefix. The 24 kHz-to-48 kHz regression checks starts inside the prefix and before the item, two identical stereo Loop passes, leading silence, zero underruns, and exact transport position. Fractional source boundaries remain rejected. App/media nextest passes 138/138 (aaadaw-app 98/98 with --no-default-features, audio_playback 26/26; aaadaw-media 40/40); strict media/app Clippy, rustfmt and git diff --check pass. The workspace cargo xtest remains blocked by the previously recorded overlay disk exhaustion. PROJECT-LOOP-001 stays in_progress: rate-converted Loop source segments beyond the bounded cache, refill, isolated CLAP routes, Repeat persistence and native audio-loop comparison remain open.

### 2026-10-15 Initial global Repeat persistence (superseded)

An initial implementation stored Repeat beside Loop/Time Selection linking in global `loop-time-preferences.json`. The native probe in the following entry showed that this scope was wrong: REAPER stores Repeat per project. The global Repeat field and its persistence test have been removed; the compatibility reader ignores that legacy JSON field.

### 2026-10-15 Match native project Repeat persistence

An isolated Linux REAPER 7.82 / Default 7 probe saved one project with Repeat on and another with Repeat off. Their RPP files contain `LOOP 1` and `LOOP 0`; after a clean REAPER restart each project restored its own value. Toggling Repeat without saving left the native project state-change count unchanged, closing without save kept the RPP value, and a new project tab started with Repeat off. Raw values and the repeatable four-phase probe are in [the fixture](../verification/reaper-parity/fixtures/loop-repeat-persistence-native.tsv) and [ReaScript](../../scripts/reaper_parity/probe-loop-repeat-persistence.lua).

AAADAW now stores Repeat in each project's Arrangement state. Schema 52 adds a checked `repeat_enabled` column with an off default for older projects; project open and recovery restore it, while explicit project saves write the current value. Toggling Repeat does not change project revision or schedule a global preference save, matching the native state-change behavior; Loop/Time Selection linking remains global. The app regression checks the Repeat action and project view-state mapping, and the storage regression covers schema 51 migration plus per-project round-trip. Storage nextest passes 88/88; app test targets pass `cargo check --tests` with and without `audio-device`; storage/app strict no-default-feature Clippy and formatting pass. The app test binary could not be linked in this workspace because the 32 GB overlay filled during nextest compilation. `PROJECT-LOOP-001` remains `in_progress`: native audio-loop equivalence, converted prefixes beyond the 1 MiB bound, feeder refill, isolated CLAP scheduling, and full visual parity remain open.


### 2026-10-15 Stream long rate-converted Loop prefixes

Rate-converted pre-Loop playback no longer rejects decoded Loop intersections above the 1 MiB cache bound. Short sections retain the bounded cyclic-history cache. Larger sections pre-scan the source on the cancellable media worker only to collect the Loop-tail sinc history, then report startup readiness and stream the linear prefix and Loop through decoder restarts. The callback still performs no decode, I/O, or allocation; the pre-scan delays playback readiness for long or compressed sources so playback cannot reach Loop before periodic history is prepared.

`stream::tests::rate_converted_pre_loop_prefix_streams_large_loop_sections_without_caching_them` drives a 24 kHz stereo source whose decoded Loop exceeds 1 MiB through two complete 48 kHz passes, then compares every sample against an independently seeded cyclic sinc reference. The media nextest suite passes 41/41, strict all-target media Clippy passes, test targets compile, formatting and diff checks pass. The parity boundary remains: fractional source-frame mappings, decoder-rate changes, ordinary-feeder backward refill, isolated CLAP scheduling, and native REAPER audio-loop equivalence remain open; long-source preparation latency is documented in [the resampling notes](../verification/reaper-parity/project-playback-resampling.md).

### 2026-10-16 Match a native REAPER MIDI export

Captured a type-1 Standard MIDI export from Linux REAPER 7.82 / Default 7 using a fresh project, one `Piano` track, and a MIDI item containing a C4 note, CC1, and pitch bend. The checked-in 119-byte native file establishes that REAPER names its conductor track from the destination filename, always writes the starting 120 BPM and 4/4 events when optional tempo/meter changes are unchecked, orders same-tick metadata as name/signature/tempo, and ends the MIDI track at its last event rather than the item's end. The repeatable native project/dialog setup and file hash are recorded in [the probe script](../../scripts/reaper_parity/probe-midi-export-content.lua) and [the fixture manifest](../verification/reaper-parity/fixtures/menu-export-midi-native-content.tsv).

Updated the encoder to match the reference bytes exactly for this Type 1 case, including a destination-derived conductor name in the export path. The byte-level regression passes with the native fixture; the package suites pass `aaadaw-core` 134/134 and `aaadaw` 647/647. Strict all-target app Clippy, rustfmt, and diff checks pass. This comparison does not close `FILE-MIDI-EXPORT-001`: Type 0, non-default tempo/meter maps and ramps, SMPTE variants, marker/cue exports, Unicode and long names, and the complete settings interaction still need native probes. The older 2026-10-15 account of rate-converted Loop sections failing above a 1 MiB bound is superseded by the entry immediately above, which removed that rejection through worker-side streaming preparation.

### 2026-10-16 Match native Type 0 MIDI export

Extended the same REAPER 7.82 fixture to `Merge to single MIDI track` (SMF Type 0), PPQN 960, with tempo/meter, SMPTE, and marker options unchecked. Native REAPER writes only the four channel events in the merged track for this case; it omits the conductor/track-name, base tempo, and time-signature events used by the Type 1 export. The 45-byte output and Type 0 dialog capture are archived in [the native file fixture](../verification/reaper-parity/fixtures/menu-export-midi-native-type0.mid), [dialog screenshot](../verification/reaper-parity/fixtures/menu-export-midi-native-type0-dialog.png), and [measurements](../verification/reaper-parity/fixtures/menu-export-midi-native-type0.tsv).

Updated the Type 0 encoder path to include only explicitly requested metadata and MIDI events, while Type 1 retains its mandatory conductor track and starting tempo/meter. A byte-for-byte Type 0 regression passes; full `aaadaw-core` nextest passes 135/135 and `aaadaw` passes 647/647. Strict all-target app Clippy, rustfmt, and diff checks pass. The rebuilt desktop binary (Build ID `de5dea7f5742d1055c8ee17c3b723939305389cd`, SHA-256 `a24ede99497d36afdb092d539393b01bfe408800bca9f7cba30f984ea11e208a`) launched on bundled Xvfb and rendered its 1280×800 main window; its startup screenshot hash matches the checked-in desktop fixture. Non-default tempo/meter maps and ramps, SMPTE, marker/cue combinations, and Unicode/long names remain open in `FILE-MIDI-EXPORT-001`.

### 2026-10-16 Match a stepped Type 1 tempo/meter map

Extended the native MIDI fixture with a step to 90 BPM and 3/4 at project tick 3840 (four quarter notes), checked `Embed project tempo/time signature changes`, and exported Type 1 at PPQN 960. REAPER emits the starting 120 BPM/4/4 at tick 0, the 90 BPM/3/4 change at tick 3840, and conductor End-of-Track at 3840; the `Piano` track still ends at its final MIDI event at tick 1440. The 133-byte file, checked dialog screenshot, and probe readings are in [the fixture manifest](../verification/reaper-parity/fixtures/menu-export-midi-native-tempo-meter.tsv) and [native file](../verification/reaper-parity/fixtures/menu-export-midi-native-tempo-meter.mid). `reaper_782_type_one_tempo_and_meter_changes_match_native_bytes` passes against it.

The full `aaadaw-core` nextest suite passes 136/136; the app suite passes 647/647. Strict core and app all-target Clippy, rustfmt, diff checks, and parity-matrix validation pass. I also corrected the earlier MIDI probes to record take-local PPQ separately from project PPQ: the existing source events were at take ticks 0/240/360/960 and project ticks 480/720/840/1440. `FILE-MIDI-EXPORT-001` remains open for tempo ramps and other maps, SMPTE, marker/cue metadata, Unicode/long track names, and remaining UI details.

### 2026-10-16 Match native linear-ramp MIDI export

Captured a Type 1 export from REAPER 7.82 / Default 7 for a 120-to-60 BPM linear ramp spanning eight quarter notes. Its conductor track writes 65 tempo events on a 120-tick grid, samples each tempo at the elapsed-time midpoint of its interval, and includes the native ramp-start and interpolation-end text markers. The default project timebase is Beats (position, length, rate), the MIDI source loops, and the item inherits that setting. REAPER exports CC1 at tick 725 and pitch bend at 846 even though the take/project QN API reports 720 and 840; the encoder now maps MIDI source events through the item's elapsed project-time span. The 661-byte [native file](../verification/reaper-parity/fixtures/menu-export-midi-native-tempo-ramp.mid), [dialog screenshot](../verification/reaper-parity/fixtures/menu-export-midi-native-tempo-ramp-dialog.png), [metadata](../verification/reaper-parity/fixtures/menu-export-midi-native-tempo-ramp.tsv), and [reproducible ReaScript](../../scripts/reaper_parity/probe-midi-export-tempo-ramp.lua) preserve the comparison. `reaper_782_type_one_linear_tempo_ramp_matches_native_bytes` verifies the entire output file byte-for-byte.

Full nextest passes `aaadaw-core` 137/137 and `aaadaw` 647/647. Strict all-target core and app Clippy, rustfmt, and diff checks pass. The rebuilt desktop binary (ELF Build ID `5bf5ffa16ed565d90a95cb9c8600490127b37c94`, SHA-256 `556a7d6c4dc05434988539b38d71ac1ddd6d0377f8733f39c5cb30ba73aa753d`) launched on bundled Xvfb :93; its actual 1280×800 startup screenshot is byte-identical to the baseline. `FILE-MIDI-EXPORT-001` remains `in_progress` for other tempo-map layouts and non-linear ramps, SMPTE, marker/cue combinations, Unicode and long names, remaining settings interactions, full File menu ordering, and UI styling/focus parity.

### 2026-10-16 Match native SMPTE offset metadata ordering

Captured a Type 1 export from Linux REAPER 7.82 / Default 7 with `Embed SMPTE offset` enabled. In the default project (30 fps, non-drop, zero offset), REAPER writes the 30 fps event `FF 54 05 60 00 00 00 00` after the destination-derived conductor name and before 4/4 and 120 BPM at tick 0. The complete 120-byte file is archived with the actual dialog screenshot and frame-rate/time-offset probe readings in [the SMPTE fixture](../verification/reaper-parity/fixtures/menu-export-midi-native-smpte.mid) and [its manifest](../verification/reaper-parity/fixtures/menu-export-midi-native-smpte.tsv).

The encoder now orders SMPTE metadata before the time signature and starting tempo while retaining the existing ramp and marker event ordering. The full native byte comparison passes; nextest passes `aaadaw-core` 138/138 and `aaadaw` 647/647, and strict all-target Clippy for both packages, rustfmt, and `git diff --check` pass. The rebuilt desktop executable (ELF Build ID `2591f86fdbedbba1660f3c04e2b571177c5818a1`, SHA-256 `2d70e8a2097b4ecfb1cfa03e617485002b50c80bef6551d16cc36bb273ad4903`) launched on bundled Xvfb :93; the real 1280×800 main window rendered and its startup screenshot matches the checked-in baseline byte-for-byte (SHA-256 `ae7790daa7323931332582152cd05219af623ac5530147835ee56910ba2ebd9b`). `FILE-MIDI-EXPORT-001` remains `in_progress`: only the zero-offset 30 fps mode has native byte coverage; other SMPTE rates/offsets, marker/cue combinations, Unicode and long names, and remaining settings/UI parity are open.

### 2026-10-16 Match native MIDI marker and cue events

Extended the REAPER 7.82 / Default 7 content probe with two visible markers: `#Intro` at project tick 0 and `Verse` at tick 960. With Type 1 / PPQN 960, marker export enabled, and tempo/meter, SMPTE, filtering, and hidden-marker options off, REAPER writes `FF 06` marker events after the conductor name, 4/4, and 120 BPM; End-of-Track is tick 960. Selecting `cues` produces the same positions and names with `FF 07` cue events. The full 133-byte marker and 130-byte cue files, their checked-state dialogs, and export measurements are recorded in the [marker fixture](../verification/reaper-parity/fixtures/menu-export-midi-native-markers.tsv) and [cue fixture](../verification/reaper-parity/fixtures/menu-export-midi-native-cues.tsv); the reproducible setup is [probe-midi-export-markers.lua](../../scripts/reaper_parity/probe-midi-export-markers.lua).

Added full-file comparisons for both event types. The MIDI export integration file passes 15/15, full `aaadaw-core` nextest passes 140/140, and `aaadaw` nextest passes 647/647. Strict all-target core and app Clippy, rustfmt, `git diff --check`, and the 100-entry / 15-column parity-matrix validation pass. `FILE-MIDI-EXPORT-001` remains `in_progress`; hidden-marker inclusion, `#` filtering, marker-ruler visibility, Type 0 marker/cue combinations, alternate tempo maps and SMPTE modes, Unicode/long names, and the rest of the settings and UI parity still need native comparison.

### 2026-10-16 Match native marker filtering and hidden-marker export

Extended the same native project with visible `#Intro` at tick 0, visible `Verse` at tick 960, and hidden `#Hidden` at tick 1920. With marker export and the `#` filter enabled but hidden-marker export off, REAPER writes only `#Intro` and ends the conductor track at tick 0. Turning on `Export hidden markers` adds `#Hidden` at tick 1920 while `Verse` remains filtered; conductor End-of-Track moves to tick 1920. The 132-byte [visible-only MIDI](../verification/reaper-parity/fixtures/menu-export-midi-native-marker-filtered.mid), 142-byte [hidden-marker MIDI](../verification/reaper-parity/fixtures/menu-export-midi-native-marker-hidden.mid), two actual dialog captures, and per-case measurements preserve the results. The repeatable [ReaScript probe](../../scripts/reaper_parity/probe-midi-export-marker-filter.lua) creates the fixture and waits for `#Hidden` to be toggled in Marker Manager before opening the export dialog.

Added full-file comparisons for both settings combinations. The MIDI export integration suite passes 17/17, full `aaadaw-core` nextest passes 142/142, and the unchanged app suite passes 647/647. Strict all-target core and app Clippy, rustfmt, `git diff --check`, and the 100-entry / 15-column parity-matrix validation pass. `FILE-MIDI-EXPORT-001` remains `in_progress` for hidden project-marker-ruler exports, filtered Cue/Type 0 combinations, other tempo/SMPTE maps, Unicode/long names, and the rest of the settings, menus, focus, and style parity.

### 2026-10-16 Match native filtered Type 0 marker and cue exports

Extended the REAPER 7.82 / Default 7 marker fixture to merged Type 0 at PPQN 960, with marker export and the `#` name filter enabled. In marker mode, REAPER emits only the `#Intro` `FF 06` event at tick 0; cue mode emits the same event as `FF 07`. Both 55-byte files contain one merged track, omit track-name/tempo/time-signature events, and then preserve the C4 note, CC1, pitch bend, note-off, and End-of-Track positions. The checked-state screenshots, per-export hashes, and exact event measurements are archived in the [Type 0 marker fixture](../verification/reaper-parity/fixtures/menu-export-midi-native-type0-markers-filtered.tsv) and [Type 0 cue fixture](../verification/reaper-parity/fixtures/menu-export-midi-native-type0-cues-filtered.tsv); [the probe script](../../scripts/reaper_parity/probe-midi-export-type0-marker-filter.lua) builds the project for either event mode.

Added full-file native comparisons for both filtered Type 0 event types. The MIDI export integration suite passes 19/19, full `aaadaw-core` nextest passes 144/144, strict all-target core Clippy, rustfmt, `git diff --check`, and the 100-entry / 15-column matrix and fixture hash validation pass. The encoder already produced the native bytes, so no production behavior needed changing. `FILE-MIDI-EXPORT-001` remains `in_progress` for marker-ruler-hidden Type 0 exports, other tempo/SMPTE maps, Unicode/long names, remaining settings combinations, File-menu ordering, and visual/focus parity.

### 2026-10-16 Match native Type 1 marker-ruler visibility export

Built a REAPER 7.82 / Default 7 project with the standard MIDI item and visible `#Intro`/`Verse` markers, then hid the entire marker ruler with action 43485. The saved RPP records `RULERLANECFG 4`. In Type 1 / PPQN 960, marker export on, and the `#` filter off, leaving `Export hidden markers` unchecked excludes both markers; enabling it includes both. The exact 134-byte and 154-byte outputs, real dialog captures, saved RPP, and event measurements are archived in the [excluded fixture](../verification/reaper-parity/fixtures/menu-export-midi-native-marker-ruler-hidden-excluded.tsv) and [included fixture](../verification/reaper-parity/fixtures/menu-export-midi-native-marker-ruler-hidden-included.tsv); [the ReaScript probe](../../scripts/reaper_parity/probe-midi-export-marker-ruler-hidden.lua) recreates the hidden ruler state.

Added exact full-file regressions for both settings states. The MIDI export integration suite passes 21/21, full `aaadaw-core` nextest passes 146/146, strict all-target core Clippy, rustfmt, `git diff --check`, and the 100-entry / 15-column matrix plus fixture hash validation pass. No encoder behavior change was required. `FILE-MIDI-EXPORT-001` remains `in_progress` for Type 0 hidden-marker inclusion, other tempo/SMPTE variants, Unicode/long names, remaining settings combinations, full File-menu order, and window styling/focus parity.

### 2026-10-16 Match a nonzero native SMPTE offset

Extended the native Type 1 SMPTE comparison to a project start offset of 10.5 seconds (00:00:10:15 at 30 fps, non-drop). The reproducible probe creates the standard MIDI fixture, saves a native RPP, sets its `PROJOFFS` value to 10.5 seconds, reopens it in REAPER, and verifies `GetProjectTimeOffset` reads back 10.5. Real Project Settings captures show `0:10.500` and Video frame rate 30. With `Embed SMPTE offset` enabled, REAPER writes `FF 54 05 60 00 0A 0F 00` after the conductor name and before time signature/tempo. The [127-byte MIDI file](../verification/reaper-parity/fixtures/menu-export-midi-native-smpte-nonzero-offset.mid), [saved project](../verification/reaper-parity/fixtures/menu-export-midi-native-smpte-nonzero-offset.rpp), [dialog and settings screenshots](../verification/reaper-parity/fixtures/menu-export-midi-native-smpte-nonzero-offset.tsv), and [probe script](../../scripts/reaper_parity/probe-midi-export-smpte-offset.lua) preserve the case.

Added a full-file native regression for the nonzero offset. The MIDI export integration suite passes 22/22, full `aaadaw-core` nextest passes 147/147, strict all-target core Clippy, rustfmt, `git diff --check`, and the parity matrix plus artifact hash validation pass. `FILE-MIDI-EXPORT-001` remains `in_progress` for alternate SMPTE frame rates/drop-frame modes, Type 0 hidden-marker inclusion, other tempo maps, Unicode/long names, remaining settings combinations, full menu order, and window styling/focus parity.

### 2026-10-16 File > Recent projects foundation

Added an MRU project list (up to 20 entries) persisted in the AAADAW config directory. Successful project opens and saves update the list; selecting an entry routes through the existing save/discard/cancel transition guard before loading. The File menu exposes Recent projects, Right expands its entries, Left returns to the parent, and Enter opens the selected path. A real AAADAW desktop session opened a copy of the first-project fixture from this list; the project title, Audio track, `melody.wav` item and persisted recent path were checked after opening. Native REAPER's File menu reference screenshot is the existing [File menu fixture](../verification/reaper-parity/fixtures/menu-file-native.png); AAADAW menu/open screenshots, keyboard sequence, config state and hashes are recorded in the [recent-project fixture](../verification/reaper-parity/fixtures/menu-file-recent-projects-aaadaw.tsv).

At the initial implementation, AAADAW expanded the child list inline while REAPER showed a cascading-menu arrow. That visual difference has since been addressed in the follow-up below. The MRU capacity and clear behavior have not been compared with REAPER; missing-path handling and multi-project tab semantics remain open. Opening a recent path replaces the single active AAADAW project after the guard, so this work does not implement REAPER's project tabs. `SHELL-MENU-KEYBOARD-001` remains `in_progress`.

The three focused recent-project regressions pass, the full desktop app suite passes 629/629, strict all-target Clippy (`-D warnings`), rustfmt, and `git diff --check` pass. The same-current-project guard recognizes an already-open locked project without prompting. The final desktop binary was rebuilt from this source, launched under Xvfb, and used to open the recent path; its SHA-256 and fresh captures are preserved in the [recent-project fixture](../verification/reaper-parity/fixtures/menu-file-recent-projects-aaadaw.tsv). The current workspace lacked GLib development metadata; compile/test/link used temporary local pkg-config metadata and the installed versioned runtime libraries. No dependency or environment workaround was added to the repository.

### 2026-10-16 Match the Recent projects cascading menu

Moved desktop recent-project entries into a separate popup to the right of File, aligned the submenu with the Recent projects parent row, and flip it to the left when the right edge has insufficient room. Desktop ↑/↓ now selects among recent paths, Right enters the submenu, Left returns to the parent, and Enter opens the selected path through the existing transition guard. The mobile/touch File menu keeps its inline list so entries remain reachable on narrow touch layouts.

A rebuilt Linux desktop binary (SHA-256 `191c02f8d88d19f8234b6e2df590a9a1a2ad364aa4e6fe72e7b4d863c21e2b04`) was exercised in Xvfb: the populated AAADAW cascade aligns with its parent, clicking the path opens the project, and the path remains at the head of persisted history. A separate real-window keyboard run opened the same path with Down ×6 → Right → Down → Enter; the [cascade capture record](../verification/reaper-parity/fixtures/menu-file-recent-projects-cascade-aaadaw.tsv) contains both mouse and keyboard screenshot hashes. The native reference confirms the parent arrow, but its fresh-profile capture has no populated path, so native child ordering, width, and open/clear behavior still need a populated REAPER comparison. Multi-project tab replacement semantics remain unimplemented.

`SHELL-MENU-KEYBOARD-001` remains `in_progress`; the automated keyboard regression covers cascade entry, path selection, parent return, and Enter activation. The three focused recent-project tests pass, the full desktop app suite passes 629/629, strict all-target Clippy (`-D warnings`), rustfmt, `git diff --check`, and the 100-row / 15-column parity-matrix validation pass.

### 2026-10-16 File > Project templates

Implemented the Linux desktop Project templates cascade and a complete `.aaadaw` template workflow. The menu loads sorted project files from the AAADAW configuration's `ProjectTemplates` directory, provides `Save project as template...`, normalizes the extension, copies managed audio assets, and refuses to overwrite an existing template. Opening a template passes through the normal unsaved-project guard and opens a private temporary working copy; the source stays unchanged, the working copy does not enter Recent projects, and ordinary Save/Save new version route through Save As. Escape cannot delete the working directory while its asynchronous load is in flight. Projects opened directly from inside `ProjectTemplates` receive the same Save protection. Writes back to the source, template folder, active working copy, or a directory chosen outside the template folder are rejected. Mobile/touch keeps an inline list; Android template saving is disabled until its SAF path can publish into the internal template directory safely.

The template regressions cover sorted loading, temporary-copy lifetime, source preservation, path/extension checks, canonical and direct-open template-path classification, a real database-backed open, refusal to overwrite its source, Save-to-Save-As routing, and a valid non-overwritable template save. The full desktop app suite passes 637 unit tests plus all 21 integration tests; strict all-target no-default-feature Clippy (`-D warnings`), formatting, and diff checks pass. A rebuilt Linux binary was driven under Xvfb by mouse and XTest keyboard input; both paths opened the populated cascade and loaded the Audio track and `melody.wav` waveform. The final executable, four screenshots, source/copy hash, keyboard sequence, and isolated Recent-project state are recorded in [the template verification note](../verification/reaper-parity/project-templates.md) and [fixture TSV](../verification/reaper-parity/fixtures/menu-file-project-templates-aaadaw.tsv).

`SHELL-PROJECT-TEMPLATE-001` and overall shell parity remain `in_progress`: REAPER's native populated template order and save dialog have not been captured, multiple project tabs are still absent, and this slice does not reproduce the full native template format/default project behavior.

### 2026-10-16 Match Type 0 hidden-marker export

Captured Linux REAPER 7.82 / Default 7 in a fresh isolated profile. The probe selected `#Hidden` in Region/Marker Manager and invoked Main action 43484; the saved project and manager screenshot confirm the marker is individually hidden. With Type 0, PPQN 960, marker export enabled, hash filtering and tempo/meter/SMPTE metadata off, I exported once with `Export hidden markers` unchecked and once checked. REAPER keeps that checkbox enabled, but both 64-byte files are identical: they contain visible `#Intro` and `Verse`, omit hidden `#Hidden`, and end at tick 1440. The two dialog screenshots, marker-manager screenshot, saved project, MIDI files, and hashes are recorded in the [native fixture manifest](../verification/reaper-parity/fixtures/menu-export-midi-native-type0-hidden.tsv).

AAADAW's first byte comparison exposed the 13-byte extra hidden-marker event. The encoder now matches REAPER's Type 0 behavior while Type 1 still includes hidden markers when requested; two full-file tests cover the unchecked and checked states. Core and desktop App nextest passed 807/807 with no skipped tests. Strict all-target Core/App Clippy (`-D warnings`), rustfmt, and `git diff --check` pass. `FILE-MIDI-EXPORT-001` remains `in_progress` for Type 0 cue/hidden combinations, other tempo-map and SMPTE modes, Unicode/long names, remaining export settings and full UI/menu parity.

### 2026-10-16 Match Type 0 hidden-cue export

Repeated the hidden-marker A/B export in Cue points mode in the same verified REAPER project. The Type 0 dialog writes cue meta-events (`FF 07`), but `Export hidden markers` remains a no-op: checked and unchecked output are both 64 bytes with SHA-256 `61024b8de96421d3c9a3efc075fae1678ad287e8abac673e1196156cd17798ae`, and neither file contains hidden `#Hidden`. Both dialog screenshots and MIDI files are added to the [native fixture manifest](../verification/reaper-parity/fixtures/menu-export-midi-native-type0-hidden.tsv).

Added exact full-file regressions for both cue checkbox states. Core and desktop App nextest passed 809/809 with no skipped tests; strict all-target Core/App Clippy (`-D warnings`), rustfmt, and `git diff --check` pass. `FILE-MIDI-EXPORT-001` remains `in_progress` for Type 0 hidden-ruler and filtered-hidden combinations, other tempo-map and SMPTE modes, Unicode/long names, remaining export settings and full UI/menu parity.

### 2026-10-16 Verify real-window File menu keyboard navigation

Drove the current Linux AAADAW desktop binary under Xvfb using XTest. `Alt+F` opened File with `New project` selected; ArrowUp moved selection to `Quit` without invoking it; Escape closed the menu. The project remained empty and stopped, and the Escape frame is byte-identical to the baseline startup screenshot. Build ID, executable hash, input sequence, and all three real-window screenshots are recorded in the [menu navigation fixture](../verification/reaper-parity/fixtures/menu-navigation-aaadaw.tsv).

This closes only the captured File-menu keyboard sequence. `SHELL-MENU-KEYBOARD-001` remains `in_progress` for complete menu command coverage/order, every menu's focus and submenu behavior, and native styling parity.


### 2026-10-16 Region Render Matrix assignment model and window

Linux REAPER 7.82 / Default 7 的隔离项目确认 `SetRegionRenderMatrix` 用可见 Region 编号索引，`EnumRegionRenderMatrix` 对 Intro 返回 Master mix/Drums/Guitar、对 Verse 返回 Bass；保存的 RPP 持久化 Master flag 和 `<REGIONRENDER>` track IDs。原厂矩阵展开后的实际窗口截图、ReaScript 和项目在[矩阵 fixture](../verification/reaper-parity/fixtures/region-render-matrix-native.png)及[交互记录](../verification/reaper-parity/fixtures/region-render-matrix-aaadaw.tsv)。

AAADAW Core 新增稳定 RegionId 对应的 Master/TrackId render targets 和 undoable `SetProjectRegionRenderMatrix`；schema 53 迁移及 SQLite 往返默认兼容旧工程。删除轨道会清理矩阵引用，撤销恢复。桌面新增 View/Action List 命令、属性窗入口、独立矩阵窗口与 Render Source `Region Render Matrix`，范围限制于 All/Selected Project Regions，候选任务进入既有原子 WAV 批次队列。新二进制加载匹配 AAADAW fixture 后实窗打开矩阵；鼠标勾选 Verse 的 Master 输出、Ctrl+Z 还原已截图。两窗口及单元格编辑/Undo 见[观察清单](../verification/reaper-parity/fixtures/region-render-matrix-aaadaw.tsv)。

Core + Storage nextest 241/241、桌面 App nextest 638/638、受影响 crates 的 strict Clippy `-D warnings`、`cargo check -p aaadaw --tests`、fmt 和 diff 检查通过。完整 workspace `cargo xtest` 本轮在链接 AAADAW 测试二进制时因 overlay 磁盘空间耗尽失败（`ld` Bus error / `No space left on device`），随后清理本轮生成的 Rust 临时产物并以受影响 crates 分别通过 nextest；未把 workspace 测试记作通过。没有修改实时音频 callback 路径。矩阵 AAADAW 使用独立窗口，REAPER 使用底部 dock；Lane/颜色/Manager 完整交互和视觉几何未完成。矩阵音频 A/B 与进一步 App WAV 回归记录在下面同日的独立进度项中；这两个 parity row 仍保持 `in_progress`。


### 2026-10-16 Region Render Matrix native audio comparison

使用新增的确定性 PCM24 source generator 和 Linux REAPER 7.82 / Default 7 ReaScript，将相同的 Drums/Bass/Guitar 素材放入 Intro `[0,2)`、Verse `[2,4)`，并设置 Intro=Master/Drums/Guitar、Verse=Bass。Native Region Render Matrix 输出恰好四个已勾选目标文件；每个是 48 kHz stereo PCM24、96,000 frames，未分配的 target/region 组合没有输出。逐文件名称、首样本、PCM payload hash、文件 hash 和可复现脚本见[原厂渲染记录](../verification/reaper-parity/fixtures/region-render-matrix-render-native.tsv)及[PCM WAV fixtures](../verification/reaper-parity/fixtures/region-render-matrix-render/)。

新增 App 回归创建同一素材/工程、构建矩阵批次并运行正式离线 WAV renderer；它断言四个目标/范围/文件名候选，并比较每个结果 WAV 的 RIFF `fmt` 和完整 `data` chunk。四个 PCM payload 与原厂 WAV 逐字节一致，等价于这组常量场景下全部 96,000 个 stereo frame 每个 sample 一致。矩阵 Master 输出现追加 `-Master` 目标后缀，与 native `$region-$track` 的目标标签相符；AAADAW 当前按用户选择的文件名前缀输出，通用命名 UI/通配符仍需实现。该 fixture 只验证无 FX/发送的直连轨道和 Master sum；fader/automation、folder/send、FX 及更广边界场景仍未覆盖，两个验收原子继续 `in_progress`。

当前桌面 App nextest 639/639、AAADAW all-target strict Clippy `-D warnings`、fmt 和 diff 检查通过。完整 workspace `cargo xtest` 仍受本机 overlay 链接空间限制，不能据此宣称 workspace 测试通过。


### 2026-10-16 Render filename Wildcards 基础

Render 窗口新增可选文件名模式字段及分组的 `Wildcards…` 插入面板，当前列出 `$project`、`$bpm`、`$tempo`、`$timesignature`、`$timesignum`、`$timesigdenom`、`$track`、`$tracknumber`、`$tracknameornumber`、`$region`、`$regionnumber`、`$marker`、`$markernumber`、`$item` 共 14 个变量，选中后追加到模式末尾并自动收起；空模式仍保留现有命名，输出路径沿用所选目录和扩展名，危险文件名字符会替换，多个任务生成重名时在入队前拒绝。Linux REAPER 7.82 Default 7 原厂实窗验证 `$tracknumber-$tracknameornumber-$regionnumber-$tempo-$timesignature-$timesignum-$timesigdenom` 输出 `01-Render fixture-02-120-4-4-4-4.wav`，`$markernumber` 输出 `01.wav`；矩阵原厂 `$region-$track` 模式的 Intro Master/Drums/Guitar 与 Verse Bass 文件名也与 AAADAW 一致，四个 96,000-frame PCM24 stereo data chunk 全部逐字节一致。AAADAW 文件名字段、展开 Wildcards 面板及实际 token 点击插入后的实窗截图、构建 ID、SHA-256 和复现脚本见 [Wildcard 验证](../verification/reaper-parity/render-filename-patterns.md)。新增回归验证 14 个 token 的列表、组合上下文展开、追加插入结果及菜单关闭；当前 App 全量测试、strict all-target App Clippy `-D warnings`、strip-debug 桌面构建、rustfmt、diff 检查、验收矩阵结构和截图 SHA-256 完成后记录于该验证文档。完整 REAPER token 集、光标/选区插入、escaping/preset/跨启动持久化、native collision prompt 和更多 source/range 对照仍未完成；`RENDER-WILDCARDS-001`、`RENDER-SOURCE-001`、`UI-028` 保持 `in_progress`。

### 2026-10-16 Render filename Project Information wildcards

在已有 Project Notes 模型上新增 `$title`、`$author`、`$projectnotes`，同时修正无名轨道的 `$tracknameornumber`：REAPER 7.82 输出 `Track NN`，AAADAW 现在复现相同形式。原厂 ReaScript 用 `PROJECT_TITLE`、`PROJECT_AUTHOR` 与 `GetSetProjectNotes` API 设置 `Wildcard Session`、`Render Author`、`Mix notes`，实窗预览生成 `Wildcard Session-Render Author-Mix notes.wav`；空名轨道的原厂预览为 `Track 01.wav`。新增的组合上下文回归覆盖 Notes 元数据，并对第二条空名轨道验证 `Track 02.wav`；App nextest 663/663 与 strict all-target App Clippy `-D warnings` 通过。17-token 面板、实际 X11 `$tracknumber` 插入和 metadata/空名轨道 REAPER 对照截图、Build ID 与 SHA-256 记录在 [Wildcard 验证](../verification/reaper-parity/render-filename-patterns.md)；rustfmt、diff、102 行唯一验收矩阵及七张截图 hash 校验通过。REAPER 的 `$projectnotes(name)` named-note 解析、项目目录变体、完整文件名交互和其余 wildcard 类别保持未完成，验收项继续 `in_progress`。


## 2026-10-16 Render Position/Length wildcards

Render filename picker 新增 `$start`、`$end`、`$length`、beat、timecode、frame、seconds 与 length-component 共 18 个 Position/Length tokens，picker 扩展到 35 项并按功能分组。REAPER 7.82 默认设置下，用 2.125–3.359 秒选中 Item 验证时钟、4/4 拍位置、30 fps timecode/frame、秒与毫秒长度输出；原厂截图及预期值已入 fixture。修复 Media items 行误收全部后续按钮造成的横向溢出；更新后的 640×720 Render 窗口完整展示各组按钮，并通过 X11 实点最底部 `$lentt` 按钮，确认写入字段后关闭 picker。

App nextest 664/664、strict all-target App Clippy `-D warnings`、stripped-debug 桌面构建通过；默认界面、全量 picker、实际 token 插入截图来自同一 Build ID，并记录在 [Wildcard 验证](../verification/reaper-parity/render-filename-patterns.md) 与 fixture manifest。格式、diff 检查、102 行唯一 ID 矩阵与全部 12 张登记截图 SHA-256 通过。项目时间偏移、跨拍号范围、非 30 fps、drop-frame、Tail 交互及 REAPER 完整 wildcard 清单仍待原厂比对，`RENDER-WILDCARDS-001`/P10 持续 `in_progress`。


## 2026-10-16 Render format/channel wildcards and native catalog

根据 REAPER 7.82 Wildcards 菜单及 help 页新增 `$format`、`$samplerate`、`$sampleratek`、`$bitdepth` 和 `$channels`，AAADAW picker 扩展到 40 项。原厂实窗预览验证 48 kHz/24-bit/stereo、44.1 kHz/mono/16-bit、22.05 kHz/mono/2-bit cADPCM，以及 IMA 4-bit；千赫兹字符串分别匹配 `48`、`44.1`、`22.05`，频道数匹配 1/2，压缩 WAV 的 bit depth 按 4/2 显示。功能回归覆盖 9 种已有 WAV sample format、44100/22050/48000 sample rates 和 Mono/Stereo。

重新组织 picker 末端为独立 Output format 与 Channels 行，并为新增的屏外组补充滚动插入路径；实点 `$sampleratek` 后字段填入正确 token 且面板关闭。40 项分组与滚动后的下半组已用当前 Build ID `33c695bb92717bccbf4a2727bcdfc47b10d62e70` 的实际桌面二进制截图。

另从原厂实窗采集九类共 97 个 wildcard 菜单项、Master $Track substitution 与目录创建选项，并记录 wildcard help 的大小写、字符替换、序号/截取、空值分隔符清理、命名 notes 与位置语义；详情及截图在[原厂 Wildcard catalog](../verification/reaper-parity/render-wildcard-catalog.md)。AAADAW 当前实现 40 项，其余原厂 token、层级 submenu 交互、语法、预设和多源/range 行为继续保持未完成。App nextest 665/665、strict all-target App Clippy `-D warnings`、stripped-debug 构建、格式检查、diff、102 项唯一验收矩阵及 36 张登记截图 SHA-256 通过；`RENDER-WILDCARDS-001`/P10 仍 `in_progress`。


## 2026-10-16 Native wildcard text rules

按 REAPER 7.82 Wildcard help 实现大小写模式（首字母大写、整体大写、整体小写）、`<X>` 删除字符、`<abcdeX>` 字符映射替换、`[<N]` 前缀截取，以及空值旁一个空格/连字符/下划线/点号的省略。新增两项 resolver 回归，覆盖这些规则；App nextest 667/667、strict all-target App Clippy `-D warnings` 通过。重建并运行 Build ID `a480f6100388ac6490b4f413814d75229198eb5f` 的桌面二进制，复拍 Render 字段、40 项 picker、滚动 picker 与 `$sampleratek` 实际点击插入界面；帮助页及菜单证据见[原厂 Wildcard catalog](../verification/reaper-parity/render-wildcard-catalog.md)。`[N]` 渲染序号、`/` 目录创建、`{lane}`、命名值表达式与其余原厂 token 仍未实现，`RENDER-WILDCARDS-001` 保持 `in_progress`。


## 2026-10-16 Media item number wildcard

补齐原厂 Media Items 菜单中的 `$itemnumber`。其值来自当前选中音频 Item 在工程 Item 序列中的一基序号，并按两位数输出；与已有 `$item` 共享同一身份索引。回归以第二个工程 Item 验证 `$item-$itemnumber` 为 `item_02-02`，与已存档的原厂第二个选中 Item `$item` 预览 `item_02` 一致。Picker 增至 41 项，Media items 组同列显示 `$item` / `$itemnumber`；X11 实点第二个按钮后正确插入 `$itemnumber` 并关闭 picker。App nextest 668/668 与 strict all-target App Clippy `-D warnings` 通过；Build ID `8d0c0fdde9a40e520786848ba56c265ab2d1a705` 的 Render、完整 picker、滚动 picker 和实点插入截图已登记。直接 `$itemnumber` 原厂输出预览及其他原厂 Media Items 项仍开放。


## 2026-10-16 Named project-note wildcard

实现原厂 Media/Project Information 菜单中的 `$projectnotes(name)`，按原厂 help 的 `name=value` 行精确取值；缺失名称为空，并复用空值相邻分隔符清理。使用原厂帮助页示例 `first=john`、`last=doe` 新增回归，模式 `$projectnotes(first)-$projectnotes(last)-$projectnotes(missing)` 得到 `john-doe`。picker 将 Project notes 拆为普通备注与 named-value 两个按钮，插入坐标脚本同步更新。Picker 扩展到 42 项；App nextest 669/669 与 strict all-target App Clippy `-D warnings` 通过。Build ID `32409702f28800989b39000cedcd7387752a9249` 的 Render、picker、滚动 picker、named-note、itemnumber 和 sampleratek 实点截图已登记；区域/标记/Item/Take marker 命名值及其余原厂 wildcard 仍开放。

### 2026-10-16 Numbered region and marker wildcards

Render Wildcards picker 新增 `$regionnumber[N]` 与 `$markernumber[N]`，总数达到 44。`N` 指定第一个输出编号，后续 Region/Marker 按序递增并保留位宽；第二个对象分别用 `[001]` 得到 `002`、用 `[01]` 得到 `02`。原厂 Wildcard help 第 3 页明确记录 Region 规则，原厂 Regions/Markers 菜单列出两种编号形式；Linux REAPER 7.82 的 All project markers 实际渲染验证 `$markernumber[005]` 生成 `marker-005.wav`、`marker-006.wav`、`marker-007.wav`，对话框与完成列表见[原厂编号 Marker fixture](../verification/reaper-parity/fixtures/render-wildcards-markernumber-all-markers.tsv)。分开 Region 和 Marker picker 行，避免四个控件横向溢出；新的全量/滚动 picker 与 `$regionnumber[N]` 实点插入截图均来自当前桌面构建。

App nextest 670/670、strict all-target App Clippy `-D warnings`、rustfmt、diff 检查及截图脚本 `bash -n` 通过。Build ID `c228aa58504ad615d9119fe433e37bbfeb221374`、SHA-256 `d13a8081aee882605c766e1ca635422f37ccbb491bc26c697e34a91754cf6255`；窗口、全量/滚动 picker、编号 Region 插入与原厂编号 Marker 对照截图哈希已写入 [Wildcard 验证记录](../verification/reaper-parity/render-filename-patterns.md) 和 fixture manifest。其他 `[N]` 渲染序号、Region 编号的直接渲染对照、命名表达式、完整编辑交互及剩余 Wildcards 仍开放，`RENDER-WILDCARDS-001`/P10 持续 `in_progress`。


### 2026-10-16 Empty Region and Marker name fallbacks

原厂 Render wildcard 帮助将 `$region` / `$marker` 定义为对象名称或 ID。Linux REAPER 7.82 用两个空名 Region、其起始空名 Marker 和 `All project regions` 实际渲染 `$region-$marker`，文件名为 `01-01.wav` 与 `02-02.wav`；预览、完成列表、输出哈希和可复现探针见[空名 Region/Marker fixture](../verification/reaper-parity/fixtures/render-wildcards-empty-name-native.tsv)。AAADAW 现在在对象存在且名称为空时回退到两位可见编号，不存在匹配对象时仍为空；新增双范围 resolver 回归覆盖该行为。App nextest 671/671、strict all-target App Clippy `-D warnings`、rustfmt、diff 检查和截图脚本语法通过。Build ID `c78a3f5245a5ea08c61a8ad42a75c710d051bb5a`、SHA-256 `62cf2ef340b75d995d7ff3503391a0c2a8263b23f55f1390feac45ba632f0c35` 的实际 Render 窗口已启动，并从该构建复拍字段、44 项 picker、滚动 picker 和插入状态；截图与两个原厂命名 fixture 的哈希已记入 [Wildcard 验证记录](../verification/reaper-parity/render-filename-patterns.md) 及 manifest。


### 2026-10-16 Region and Marker name wildcards

将 REAPER 7.82 Regions/Markers 菜单中的 `$regionname`、`$markername` 加入解析器和 Wildcards picker，当前为 46 项。两者只返回匹配对象的原始名称；空名保持空，不回退为对象编号，旁边的分隔符依原厂帮助规则移除。Linux REAPER 7.82 `All project regions` 实际渲染 `$regionname-$markername` 得到 `Verse-Start.wav`、`Chorus-Next.wav`；未命名对象模式 `$regionname-$regionnumber` 预览并渲染为 `01.wav`、`02.wav`。原厂对话框、完成列表、输出哈希和空名行为截图见[名称 wildcard fixture](../verification/reaper-parity/fixtures/render-wildcards-region-name-native.tsv)。新增回归覆盖两个命名范围及空名时 Region/Marker name token 的无回退行为。

App nextest 672/672、strict all-target App Clippy `-D warnings`、rustfmt、diff 检查及 picker capture 脚本语法通过。Build ID `f62ddf91a3a8c729fd70ab18049948aa2531ad52`、SHA-256 `c5c4ac655fdeeffd5c3fb7ee3e0e77b01e70b808858963a43103421bb0652c6b` 的桌面构建通过实窗截图复核；46 项 picker 与滚动区域、`$regionname` / `$markername` 实点插入和 picker 关闭状态均来自该构建。原厂 named/unnamed 预览和当前 AAADAW 界面截图哈希写入 [Wildcard 验证记录](../verification/reaper-parity/render-filename-patterns.md) 与 fixture manifest。`$region(name)`、`$marker(name)` 表达式及其他未实现原厂 wildcards 仍保持开放，`RENDER-WILDCARDS-001`/P10 继续 `in_progress`。

实现 `$region(name)` 与 `$marker(name)`，从匹配渲染 Region/Marker 的名称中按 key 精确读取 `name=value`，缺失键返回空；按原厂 `$region(name)[s]` / `$marker(name)[s]` 语义用单字符分隔 `name=value` 项，并可读取后续匹配键；省略后缀时首个值保留后续文本。REAPER 7.82 实窗 `[|]` 对照预览并渲染 `Verse-Start.wav`、`Chorus-Next.wav`，无后缀预览确认 `|` 后内容仍在值中；截图、PCM24 输出尺寸和 SHA-256 记入 [named-value fixture](../verification/reaper-parity/fixtures/render-wildcards-region-named-values-native.tsv) 与 [second-key fixture](../verification/reaper-parity/fixtures/render-wildcards-region-named-values-second-key-native.tsv)。Picker 扩展为 48 项，Region/Marker values 分组的按钮实点插入并关闭状态已由新桌面 Build ID `49b1bac712095188592ac618749d45a80ff13de0`、SHA-256 `25332b55dfae6938ea579a0ccf0b8ca6bd58f5b89171320f2f6b658f13c4e72f` 截图确认。App nextest 674/674、strict all-target App Clippy `-D warnings`、rustfmt 与 diff 检查通过；Item/Take 命名值、其他原厂 wildcard、光标/选区插入与预设持久化等继续开放，P10 / `RENDER-WILDCARDS-001` 保持 `in_progress`。


### 2026-10-16 Native direct Item number render

用 Linux REAPER 7.82 的 `Selected media items` source 单独选中三个工程 Item 中的第二个，将 pattern 设为 `$itemnumber`。原厂预览指向 `02.wav`，实际渲染也生成 `02.wav`（48,000 帧、48 kHz stereo PCM24）；渲染对话框、完成列表、源项目、输出哈希与可复现 probe 见[原厂 Item number fixture](../verification/reaper-parity/fixtures/render-wildcards-itemnumber-native.tsv)。这补上了此前只从 `$item` 输出确认序号、但尚未单独渲染 `$itemnumber` 的差异。AAADAW 现有第二个 Item 的 `$itemnumber` 回归值 `02` 与原厂一致；App nextest 672/672、strict all-target App Clippy 及桌面构建结果沿用上方同一代码版本，Render wildcard 验收仍 `in_progress`。


### 2026-10-16 Native `$tracknamenumber` picker token

將 Tracks picker 的 `$tracknameornumber` 文字修正為 REAPER 7.82 Project Information 菜單使用的 `$tracknamenumber`，並保留舊拼法作為解析器兼容別名。原廠 `Selected tracks (stems)` 對空名首軌以 `$tracknamenumber` 預覽並實際生成 `Track 01.wav`（48 kHz、stereo、PCM24、240,000 frames）；原廠預覽/完成列表、WAV 哈希及探針記入 [native fixture](../verification/reaper-parity/fixtures/render-wildcards-tracknamenumber-native.tsv)。新桌面 Build ID `c8737e71c4c13fff201e069c653e602b5b54f20e`、SHA-256 `93bb0479cce315090bb9731590ab5f6f7168985eaa3ed3e8192259ab21cd6538` 的實窗截圖確認 picker 顯示 `$tracknamenumber`，實點插入後輸入框內容正確且 picker 關閉，见 [AAADAW insertion capture](../verification/reaper-parity/fixtures/render-filename-pattern-wildcard-tracknamenumber-aaadaw.png)。完整 App nextest 674/674、strict all-target App Clippy `-D warnings`、rustfmt、shell syntax 和 diff 检查通过。Picker 仍保持 48 个已实现 token，目录/层级与其他原厂变量继续开放，`RENDER-WILDCARDS-001`/P10 继续 `in_progress`。

### 2026-10-16 Project-directory render wildcards

在 Render Wildcards 中加入 `$projectdirectory` 和 `$projectdirectoryN`，并在 picker 增加 Project directories 分组。Linux REAPER 7.82 Default 7 对嵌套工程 `ProjectRoot/Albums/Album01/Session/Directory Session.rpp` 的原厂预览确认当前目录 `$projectdirectory` / `$projectdirectory1` 为 `Session`，层级 2/3 为 `Album01` / `Albums`；在更深路径中层级 4–7 为 `ProjectRoot-capture-render-wildcards-project-directory-reaper782`。实窗还确认 `$projectdirectory0` 输出 `Session0`（数字零保留为字面后缀），而菜单模板 `$projectdirectoryN` 输出 `SessionN`。PCM24 stereo 48 kHz、48,000-frame 的实际原厂渲染文件为 `Session-Session-Album01-Albums.wav`。原厂预览、成品截图、WAV 与可复现 ReaScript 记录在[项目目录 wildcard fixture](../verification/reaper-parity/fixtures/render-wildcards-project-directory-native.tsv)。

解析器沿已保存工程路径向上取父目录组件；正整数后缀选择相应层级，零后缀和大写 `N` 模板保留字面字符。新增回归覆盖层级 1–6、超出根目录的空值、`0` 与 `N`；picker token 列表为 50 项，分组切片和插入坐标同步调整。Build ID `9fd02f7e28e0dbc7f14a023eea700350445947dd`、SHA-256 `15e31d74c0b02c9ab88acbaf160d6a5e8e8252bbb4aed806989857f5a75177f0` 的新桌面二进制已实际启动；更新后的完整/滚动 picker 截图及两个目录按钮分别插入、关闭 picker 的截图哈希均写入 [Wildcard 验证](../verification/reaper-parity/render-filename-patterns.md) 和 fixture manifest。AAADAW App nextest 654/654、strict all-target Clippy `-D warnings`、rustfmt、diff 检查和捕获脚本 `bash -n` 通过。P10 / `RENDER-WILDCARDS-001` 仍 `in_progress`：其余原厂 token、光标/选区插入、目录创建语法、预设持久化与完整 REAPER 窗口/源范围交互尚未完成。

### 2026-10-16 Render track FX wildcards

加入 `$fx` 和 `$fx[X]` 两种 track FX 文件名 wildcard，并把 Track FX 按钮组加入 Render picker。Linux REAPER 7.82 / Default 7 对带有 `JS: Volume Adjustment` 与 `ReaEQ (Cockos)` 的轨道，以 Selected tracks (stems) 实际预览并渲染 `$fx` 为 `Volume Adjustment_ReaEQ`、`$fx[,]` 为 `Volume Adjustment,ReaEQ`；PCM24 输出文件、原厂预览/完成截图及哈希记录在 [FX native fixture](../verification/reaper-parity/fixtures/render-wildcards-fx-native.tsv)。AAADAW 使用渲染轨道上可解析 CLAP 插件的显示名称，默认 `_` 分隔，`[X]` 内单字符作为自定义分隔符；52 项 picker、滚动 Track FX 行和两个 wildcard 的实点插入截图及 Build ID / 二进制哈希记录在 [验证文档](../verification/reaper-parity/render-filename-patterns.md) 与 fixture manifest。

App nextest 655/655、strict all-target App Clippy `-D warnings`、rustfmt、diff 检查和 picker capture 脚本 `bash -n` 通过。原厂对照目前覆盖 Selected tracks (stems)；Master、Selected media items、缺失插件和更复杂 FX 命名情形仍需 REAPER 比较，`RENDER-WILDCARDS-001`/P10 及其余 native wildcard / Render 交互保持 `in_progress`。


### 2026-10-16 Render track hierarchy wildcards

新增 `$trackslashes`、`$folders`、`$folders[X]` 和 `$parent`，picker 达到 56 个 token。Linux REAPER 7.82 / Default 7 以 `Drums > Acoustic > Kick/Snare` 的 Selected tracks (stems) fixture 验证：`$track` 将斜杠转为 `_`，`$trackslashes` 保留斜杠作为强制目录分隔符，`$folders` 按外层到内层连接父文件夹，`$folders[,]` 使用逗号，`$parent` 返回直接父文件夹；原厂预览和完成渲染、PCM24 WAV 及哈希保存在[层级 wildcard fixture](../verification/reaper-parity/fixtures/render-wildcards-track-folders-native.tsv)。AAADAW 单文件与 stem 批次现支持各自的嵌套输出目录，失败或取消时删除本次新建且为空的目录；回归覆盖层级值、根轨道空父级、不同 stem 子目录以及取消清理。全量/滚动 picker 与四个实际 token 插入截图、桌面构建 ID 和 SHA-256 记入 [Wildcard 验证记录](../verification/reaper-parity/render-filename-patterns.md) 与 fixture manifest。验证：AAADAW App nextest 656/656，`aaadaw-app` nextest 201/201，`aaadaw` 与 `aaadaw-app` all-target strict Clippy `-D warnings`、rustfmt、`git diff --check`、截图捕获脚本 `bash -n` 均通过；登记的六张 AAADAW 截图、两张 REAPER 截图及两边 WAV SHA-256 均已重算匹配。一般 `Allow /` 选项及跨启动持久化仍开放；`RENDER-WILDCARDS-001`/P10 继续 `in_progress`。

### 2026-10-16 Render selected-track wildcard

实现 `$seltrack` 并把它加入 Tracks picker。Linux REAPER 7.82 / Default 7 选择 Bass 与 Keys 后，Selected tracks (stems) 的 `$seltrack-$track` 输出为 `Bass-Bass.wav`、`Bass-Keys.wav`，其中 `$seltrack` 取工程轨道顺序中第一个选中轨道，`$track` 随各 stem 变化；仅选 Keys 的 Master mix 输出为 `Keys-Master.wav`。原厂预览、成品列表、三份 PCM24 WAV、哈希和复现脚本在[选中轨道 fixture](../verification/reaper-parity/fixtures/render-wildcards-seltrack-native.tsv)。AAADAW 在提交 Render job 时将当前轨道选择上下文传入文件名展开；回归覆盖 Keys-only Master、两个 stem 的 primary selection 保持不变，以及无选中轨道时相邻分隔符清理。

57 项 picker 与实际 `$seltrack` 点击插入截图由桌面 Build ID `7cba54a122ea96c879d7d1d645d7d7c17434a4a8`、二进制 SHA-256 `d30b5f110c6e491bf81d497ab4e108ba75e002471b14ac18c03d0058ba937dc1` 实际启动后复核，截图 hash 记入 [Wildcard 验证记录](../verification/reaper-parity/render-filename-patterns.md) 与 fixture manifest。完整 `cargo xtest -p aaadaw` 1258/1258、目标回归、`aaadaw` all-target strict Clippy `-D warnings`、rustfmt、diff 检查和截图脚本语法通过；`RENDER-WILDCARDS-001`/P10 继续 `in_progress`，可调 `$playrate`/Master transport 速度、剩余原厂 token、光标/选区插入、预设/跨启动持久化、碰撞提示和更多来源/范围行为仍待完成。


### 2026-10-16 Render `$playrate` wildcard

将 `$playrate` 加入 Render Wildcards picker，当前共 58 个 token。Linux REAPER 7.82 / Default 7 的临时工程先用 Transport action 40521 重置 1x，再用 40522 增加一个半音，Master rate 读回 `1.059463094`；`$playrate-$track` 预览并实际渲染为 `1.059-Master.wav`。48 kHz stereo PCM24 成品为 45,306 帧、0.943875 秒，确认原厂按三位小数展开且非 1x 会改变离线渲染时长；复现脚本、预览、完成窗口、WAV 和 SHA-256 在[原厂 playrate fixture](../verification/reaper-parity/fixtures/render-wildcards-playrate-native.tsv)。

AAADAW 当前还没有可调 Master transport rate，故项目离线渲染始终 1x，新 token 目前展开为 `1.000`；回归验证默认输出和三位小数格式（含原厂 `1.059` 值）。58-token picker 与实际 `$playrate` 插入来自 Build ID `60ccc8ca9db91d93dbfe7e836ea33429d62e18e5`、SHA-256 `6792eab78a3cca8a93b4441796bdd98755dc490de899117467debd1a7f7a94b7` 的桌面二进制，截图哈希列在 fixture manifest。

完整 `cargo xtest -p aaadaw` 1259/1259、`aaadaw` all-target strict Clippy `-D warnings`、rustfmt、diff 检查、capture 脚本语法及截图/原厂 WAV 哈希核对通过。非默认 Master rate、transport 控件/工程持久化、实时与离线变速播放尚未实现，不能视为 `$playrate` 全语义完成；其余 native wildcard、编辑插入与 preset/persistence 也保持开放，`RENDER-WILDCARDS-001`/P10 继续 `in_progress`。

### 2026-10-16 Render system-information wildcards

加入 `$user` 和 `$computer` 文件名通配符。AAADAW 按 `USER`、`LOGNAME`、`USERNAME` 顺序取用户值；计算机名按 `HOSTNAME`、`COMPUTERNAME` 顺序取值，并回退到 `/etc/hostname`。两者经过现有安全文件名展开逻辑。Render picker 新增 Computer information 分组，当前共 60 个可插入 token。

Linux REAPER 7.82 / Default 7 使用隔离用户与 UTS hostname namespace、通用值 `FixtureUser` / `StudioComputer`，确认 `$user-$computer-$track` 预览和完成渲染均为 `FixtureUser-StudioComputer-Master.wav`；48 kHz stereo PCM24 输出为 48,000 帧。原厂预览、完成列表、WAV、SHA-256 和探针记入[原厂系统信息 fixture](../verification/reaper-parity/fixtures/render-wildcards-system-information-native.tsv)。最终 AAADAW 桌面二进制 Build ID `e4e98a5eb591f05c7bbe72bd2312655eee5e1023`、SHA-256 `c99bb944f49db61276b9fd224116f532530722a8ac54f06a5c8dade3b3987445` 已实际启动；完整 picker、滚动 picker、`$user` 与 `$computer` 实点插入截图及哈希登记在 [Wildcard 验证记录](../verification/reaper-parity/render-filename-patterns.md) 和 fixture manifest。

最终源码验证通过：`cargo xtest -p aaadaw` 1260/1260、`cargo clippy -p aaadaw --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`git diff --check`、截图脚本 `bash -n`，以及 native / AAADAW 图片与 WAV 哈希核验。`$playrate` 非默认传输速度和其余原厂变量仍未完成；`RENDER-WILDCARDS-001` / P10 持续 `in_progress`。

### 2026-10-16 Render `$uniqueid[N]` wildcard

为 REAPER 7.82 Render Wildcards 增加 `$uniqueid[N]`，picker 现有 76 项。原厂预览和实际渲染验证值为小写十六进制，省略长度时默认为 8，长度请求按 8–16 截断；每次出现分别生成 ID，字面模板 `$uniqueid[N]` 为空并触发相邻分隔符清理。精确测试的边界值、原厂预览/完成截图、两份 PCM24 WAV 及哈希保存在[unique ID fixture](../verification/reaper-parity/fixtures/render-wildcards-uniqueid-native.tsv)。

AAADAW resolver 为每个表达式生成独立 ID，并回归覆盖默认长度、8–16 下限/上限、重复项唯一性、小写格式及 `[N]` 空模板；picker 的 Unique ID 分组和 X11 实点插入分别由 Build ID `bf339f42b4f8bfba05d3e3020ed03639859b69b2`、SHA-256 `aa9a2eadd05687306cd65e5bfa50e981bf90e98f50ede940863f3b9c3040fb09` 的实际桌面构建截图验证，哈希记录在 [Wildcard 验证文档](../verification/reaper-parity/render-filename-patterns.md) 与 fixture manifest。完整 AAADAW workspace nextest 1261/1261、strict all-target Clippy `-D warnings`、rustfmt、diff 检查、截图脚本语法和登记的 native/AAADAW fixture 哈希均通过。`$uniqueid[N]` 已实现；其余原厂 wildcard、光标/选区插入、通用 Allow `/`、预设持久化、更多来源/范围与完整 Render 窗口交互仍开放，`RENDER-WILDCARDS-001` / P10 保持 `in_progress`。

### 2026-10-16 Render wildcard slash directories

实现 REAPER Wildcards > Options > `Allow / (create directory) in wildcard substitutions`。Linux REAPER 7.82 的新配置默认勾选；`$track` 值 `Kick/Snare` 在启用时预览并渲染为 `Kick/Snare.wav`，关闭时预览为 `Kick_Snare.wav`。关闭后退出并用同一隔离配置重启，菜单选项保持未勾选；`$trackslashes` 和 `$folders` 即使总选项关闭仍创建目录。原厂 fresh-default、两种预览、成品、PCM24 WAV、重启菜单截图与哈希记录在[原厂 allow-slash fixture](../verification/reaper-parity/fixtures/render-wildcards-allow-slash-native.tsv)。

AAADAW 的 Wildcards picker 新增 Options 复选框，默认开启；每次点击将版本化设置原子写入 `$XDG_CONFIG_HOME/aaadaw/render-wildcard-preferences.json`。用隔离 XDG 配置实点关闭、重启程序后截图仍为未勾选，且 resolver 仅将普通 wildcard 展开值中的 `/` 转为安全目录分隔符；强制目录 token 不受禁用状态影响。Build ID `ff4fbbf05678e018322ca36c044a1b9ca8038e3a` 的 checked / restarted-unchecked 截图、配置值和 hash 记入 [Wildcard 验证记录](../verification/reaper-parity/render-filename-patterns.md) 与 fixture manifest。`验收：AAADAW workspace nextest 1264/1264、all-target strict Clippy `-D warnings`、`cargo fmt --all -- --check`、`git diff --check`、截图脚本 `bash -n` 与登记图片/WAV SHA-256 检查全部通过。`RENDER-WILDCARDS-001` / P10 仍为 `in_progress`；其它 wildcard 语义和 Render 差异继续逐项处理。

### 2026-10-16 Render file-number and file-count wildcards

为 Render 文件名 Wildcards 增加 `$filenumber`、`$filenumber[N]` 和 `$filecount`，picker 从 76 项扩展到 79 项，并新增 Recording 分组。AAADAW 在每个待渲染输出批次中按生成顺序从 1 开始编号，`[N]` 的数字个数作为零填充宽度，总数对批次内每个文件都相同；回归覆盖两个输出的无填充/两位填充结果，以及单文件批次的 `001` 和 count=1。

Linux REAPER 7.82 / Default 7 使用 All project regions 渲染 Verse、Chorus 两个 region：`file-$filenumber[01]-$filecount-$region` 预览和完成输出为 `file-01-2-Verse.wav`、`file-02-2-Chorus.wav`；去掉宽度后为 `file-1-2-Verse.wav`、`file-2-2-Chorus.wav`。四张原厂预览/完成截图、PCM24 WAV 和 SHA-256 记录在[原厂 file-counter fixture](../verification/reaper-parity/fixtures/render-wildcards-file-counters-native.tsv)。

最终桌面构建 Build ID `19207d803b3ead43ff9a6029af23d378b660c6b5`、二进制 SHA-256 `23840c4f7a5368f7c7e98099ff272bec2deb8247cf1314c829f24e552c9d0dc1` 已实际运行；Recording 分组截图和实点 `$filenumber[N]` 插入截图及哈希记录在 [Wildcard 验证记录](../verification/reaper-parity/render-filename-patterns.md) 与 fixture manifest。`cargo xtest -p aaadaw` 全量 1265/1265、`cargo clippy -p aaadaw --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`git diff --check`、截图脚本 `bash -n`，以及新增 native/AAADAW PNG 和 PCM24 WAV 的 SHA-256 检查均通过。`RENDER-WILDCARDS-001` / P10 继续 `in_progress`；`$note`、`$natural`、`$namenumber`、timeline-order tokens、caret/selection 插入、presets、跨启动模式持久化和其它 Render 行为仍未完成。

### 2026-10-16 Render timeline-order wildcards

加入 `$timelineorder`、`$timelineorder[N]`、`$timelineorder_track` 和 `$timelineorder_track[N]`，Render picker 扩展到 83 项，并为 Timeline order 与 Timeline order by track 增加独立分组。AAADAW 根据匹配的 Region/媒体项在项目中的时间位置计算序号，而不是依赖当前渲染队列的排列；媒体项的 `_track` 形式只计算同轨项目，并保留原厂 `[N]` 起始编号和零填充宽度规则。

Linux REAPER 7.82 / Default 7 的 All project regions 预览与实际完成渲染确认 Verse/Chorus 分别展开为 `1-005-01` 和 `2-006-02`，对应 `$timelineorder`、`$timelineorder[005]` 与 `$timelineorder_track[01]`。预览、完成列表、两个 PCM24 WAV 和 SHA-256 保存在[原厂 timeline-order fixture](../verification/reaper-parity/fixtures/render-wildcards-timeline-order-native.tsv)。

最终 AAADAW 桌面 Build ID `91021083969acc5430035fb35ebb4edcd8b91bfd`、二进制 SHA-256 `57afa64c721d8315b665709025284afd01bc3b4e956b8e8ea45121ddb9b1bdd5` 的 picker 和 `$timelineorder_track[N]` 实点插入截图及哈希已登记在 Wildcard 文档与 fixture manifest。两个回归分别覆盖逆序输出批次的 Region 时间排序/起始位数和 Selected Media Items 的全局/同轨排序。AAADAW workspace nextest 1267/1267、strict all-target Clippy `-D warnings`、格式与 diff 检查、截图脚本语法和新增 fixture hash 检查通过。`RENDER-WILDCARDS-001` / P10 继续 `in_progress`；`$note`、`$natural`、`$namenumber`、`{lane}`、预设和插入/持久化行为仍待实现。


### 2026-10-16 Render recording-order wildcards

加入 `$note`、`$note[X]`、`$natural` 和 `$namenumber`，Render Wildcards picker 扩展为 87 项。录音音高通配符按本次输出批次次序递增：`$note` 从 REAPER 实际默认输出的 C-1 开始逐半音上升，`$note[B2]` 从 B2 开始，字面 `$note[X]` 首个输出为空、后续从默认 C-1 开始递增，`$natural` 只遍历 C/D/E/F/G/A/B 自然音。区域重名时 `$namenumber` 以两位数编号，目前 Region 仅在 pattern 含 `$region` 名称 token 时计算重名编号；活动 Item 名称编号仍未实现。

Linux REAPER 7.82 / Default 7 原厂探针用八个同名 Verse 区域确认预览与完成输出为 `Verse-01-C-1-B2-C-1.wav` 至 `Verse-08-G-1-F#3-C0.wav`；8 个 PCM24 WAV、预览/完成截图、空模板上下文截图及 SHA-256 均存于 [recording-order fixture](../verification/reaper-parity/fixtures/render-wildcards-recording-order-native.tsv)。帮助第 5 页将首音描述为 C0，而这台 Default 7 配置实际输出 C-1；当前遵循实测结果，并将 octave-label preference 核对保留为开放项。重复活动 Item 名称尚不可计数，因为 AAADAW 项目模型还没有 Item/Take name。

当前桌面 Build ID `c2784759262e4699dbca9cec2ac9765cea514a79`、binary SHA-256 `f2ad5c466a4d2cf656e37adbae339f8845e6871e0d5cf405b5b95ec61e894ac6` 已运行并做实际 X11 点击；完整 87 项 chooser 以及 `$note[X]`、`$note`、`$natural`、`$namenumber` 插入截图已登记在 [Wildcard 验收文档](../verification/reaper-parity/render-filename-patterns.md) 和 fixture manifest。

resolver 回归覆盖八个连续重复区域、三个音高形式、原厂字面 `$note[X]` 的八文件预览序列，以及无 `$region`/`$item` 上下文的 `$namenumber` 空值。最终 `cargo xtest -p aaadaw` 1268/1268、strict all-target Clippy `-D warnings`、`cargo fmt --all -- --check`、`git diff --check`、截图脚本 `bash -n` 以及 native WAV/PNG 和 AAADAW 截图的登记哈希均通过。`RENDER-WILDCARDS-001` / P10 继续 `in_progress`；活动 Item 名称、Help C0 与实际 C-1 差异、`{lane}`、预设与更多 Render 语义仍待实测实现。

### 2026-10-16 Audio Take name 与 `$namenumber`

Audio Item Properties 新增与 REAPER 原厂文案一致的 **Take name** 字段；名称通过 Core action、snapshot 和 schema 54 持久化，支持撤销/重做、清空恢复 source label，并在时间线与 `$item` 中使用。`$namenumber` 对所选音频 Item 渲染批次内的同名 Take 计数；空 Take name 仍遵循原厂 `item_NN` 显示/命名回退。Media Item Properties 尚有多 Take/comp、item timebase、播放速率/音高、通道、envelope、stretch、source controls 和批量字段等差距。

Linux REAPER 7.82 / Default 7 原厂实测将三个 Take name 都设为 `Hit`，只选择第一与第三个 Item，Source 设为 Selected media items，`$item-$namenumber` 的预览及完成结果为 `Hit-01.wav`、`Hit-02.wav`，证明未选中的同名 Item 不消耗序号。交叉模式 `$item-$region-$namenumber` 的原厂预览/完成文件则为 `Hit-Region 1-01.wav` 与 `Hit-Region 3-01.wav`，确认两种 name token 同时出现时 `$namenumber` 绑定 Region；AAADAW 的 resolver 回归覆盖该优先级。预览/完成截图、两份 PCM24 WAV 与 SHA-256 记录在 [Take name 原厂 fixture](../verification/reaper-parity/fixtures/render-item-take-name-native.tsv)；AAADAW 当前桌面 [Take name 属性截图](../verification/reaper-parity/fixtures/audio-item-properties-take-name-aaadaw.png)与二进制 Build ID/SHA 见[验收文档](../verification/reaper-parity/audio-item-take-name.md)。

Core/Storage 测试、App 二进制测试 669/669、strict all-target App Clippy `-D warnings`、`cargo fmt --check` 与 `git diff --check` 通过；`ITEM-PROPERTIES-001` 与 `RENDER-WILDCARDS-001` 继续 `in_progress`。当前 audio item 仅表示一个 active Take；所选 MIDI Item 的 `$item`/`$namenumber` 和更完整属性窗口仍待实现及原厂验证。


### 2026-10-16 Selected MIDI-item rendering and wildcard context

Selected Media Items now accepts Audio and MIDI items in one render batch, preserves project/track chronology for jobs, and expands `$item`, `$itemnumber`, `$namenumber`, track context, and timeline-order values for either item type. MIDI output snapshots retain only the selected MIDI item's events, clear audio items and frozen-item state, and render the item's tempo-mapped start/end range through the offline project graph; the project itself is unchanged. The regression writes a 48 kHz WAV with exactly 24,000 frames for a selected 0.5-second MIDI item, and additional App tests cover MIDI-only selection, mixed-source ordering, and wildcard naming.

A rebuilt AAADAW desktop binary was launched under an isolated Xvfb session with a three-item MIDI fixture. A real click selected one item; the Render window visibly reported `Selected media items`, `1 media item`, and that MIDI uses its track instrument. The screenshot, build ID, binary SHA-256, capture environment, and screenshot hash are in [the AAADAW fixture](../verification/reaper-parity/fixtures/render-selected-midi-items-aaadaw.tsv). Native REAPER MIDI instrument routing and ReaSynth sample comparison could not be captured in this environment, so `RENDER-SOURCE-001`, `RENDER-WILDCARDS-001`, `ITEM-PROPERTIES-001`, and P10 remain `in_progress`. Validation for this change: workspace `cargo xtest -p aaadaw-app -p aaadaw` passed all 1,275 tests across 73 binaries under bundled Xvfb; strict all-target Clippy `-D warnings`; `cargo fmt --all -- --check`; `git diff --check`; capture-script `bash -n`; matrix status and screenshot SHA-256 checks passed.


### 2026-10-16 Native Selected media items MIDI render

Installed the official REAPER 7.82 Linux x86_64 archive into `/tmp` and ran it under an isolated Xvfb/profile with the Default 7 theme. The ReaScript fixture creates a ReaSynth instrument track and three one-second MIDI items with the same take name; it selects the first and third, saves a native RPP, and opens the Render dialog. Selecting `Selected media items` in the visible dialog and rendering `$item-$namenumber-$itemnumber` produced `MIDI Hit-01-01.wav` and `MIDI Hit-02-03.wav`, confirming the unselected middle item does not consume `$namenumber`, while `$itemnumber` follows project order. Both files are stereo 48 kHz PCM24, exactly 48,000 frames. Their PCM payloads are byte-identical, peak at −8.0761 dBFS, and measure −14.0949 dBFS RMS. The native project, render dialog and finished screenshots, WAVs, and hashes are in [the native selected-MIDI fixture](../verification/reaper-parity/fixtures/render-selected-midi-items-native.tsv).

AAADAW now has a direct regression for the same first/third selection and expected filenames. It does not yet compare ReaSynth PCM because AAADAW does not load REAPER's bundled VST2 plugin; broader instrument/plugin routing and the remaining Render source modes remain open. `RENDER-SOURCE-001`, `RENDER-WILDCARDS-001`, `ITEM-PROPERTIES-001`, and P10 remain `in_progress`.

Validation follow-up: `render_filename_selected_midi_item_name_number_ignores_unselected_duplicate` passes 1/1 against the native first/third item names; strict all-target `aaadaw`/`aaadaw-app` Clippy `-D warnings`, formatting, diff, script syntax, and native/AAADAW fixture SHA-256 checks pass. The full workspace Xvfb suite passed 1,275/1,275 before this new focused regression.


### 2026-10-16 Audio item loop-copy mix and Take-name properties

Linux REAPER 7.82 / Default 7 的属性探针确认，Copy loop/Paste 保留 Audio Item 音量以及 active Take 音量、声像和播放率；本探针的完整项与起始边缘选区都把 pasted fade-in/out 重置为 0。播放率 1.25 时，3–7 秒时间选区对应源区段 3.75–8.75 秒，粘贴长度仍为 4 秒。原始探针结果 SHA-256、具体属性和环境记录在[原厂属性 fixture](../verification/reaper-parity/fixtures/item-copy-loop-properties-native.tsv)；这是行为探针，本轮没有新增截图。

另一个 REAPER 7.82 探针发现：将 `Lead phrase` Take 的选区粘贴后，名称成为 `Lead phrase loop`；空 Take 名仍为空；再次复制已带 ` loop` 的项不会增加第二个后缀。结果与脚本哈希记录在[Take-name fixture](../verification/reaper-parity/fixtures/item-copy-loop-name-native.tsv)。

AAADAW 音频剪贴板现在记录 Take 名称、Item 音量与 active Take 音量/声像；粘贴命名 Take 时按原厂规则追加一次 ` loop` 后缀，同时保留空名和已经带后缀的名称。Core 插入动作将 looped item 创建、名称及混音值写入同一原子 Undo 事务。多轨复制回归检查不同名称和混音值跨复制、粘贴、Undo/Redo 和第二次粘贴仍一致；新项的 0 淡化与探针结果一致。模型尚无 Take 播放率字段，因此 1.25x 下源区段的速率映射仍未实现；跨工程媒体剪贴板、自动创建目标轨道、跨既有 source-loop 边界复制和完整窗口/焦点验收继续开放，`ITEM-AUDIO-COPYLOOP-001` 保持 `in_progress`。

混音属性切片的完整 `cargo xtest -p aaadaw` 在本地 Xvfb `DISPLAY=:98` 下通过 1276/1276、0 skipped。随后加入 Take 名称后，AAADAW 主 binary tests 通过 671/671，`cargo test -p aaadaw-core` 的全部测试 binary 均通过，Copy loop 定向回归也再次通过 1/1；Stable `cargo clippy -p aaadaw-core -p aaadaw --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`git diff --check` 通过。Parity matrix 仍将该条目标记为 `in_progress`，因为非 1x Take 速率映射和完整原厂/AAADAW 交互范围尚未覆盖。


### 2026-10-16 Audio loop-copy destination tracks

Linux REAPER 7.82 / Default 7 探针确认 Copy loop 后即使删除了来源轨道，Paste 仍按复制片段的相对偏移自动创建空白轨道；工程没有任何轨道时粘贴两项会创建两条空轨道，并选择新项目。实测结果与脚本 hash 记录在[destination-track fixture](../verification/reaper-parity/fixtures/item-copy-loop-track-growth-native.tsv)。本轮是 ReaScript 行为探针，没有新增截图。

AAADAW Paste 现在计算缺少的目标轨道数，使用 Core 暴露的下一个单调 TrackId 构造 CreateTrack 操作，再与 looped-item 插入合并到同一 BatchTransaction；包括“只剩一轨”和“零轨”两种情况。一个回归覆盖删除来源轨道后的缺口补建、零轨道粘贴，以及 Undo/Redo 同时移除/恢复新轨道与素材。`ITEM-AUDIO-COPYLOOP-001` 仍为 `in_progress`：非 1x Take 速率映射、跨工程媒体剪贴板、跨既有 source-loop 边界复制和完整视觉/焦点对照待完成。

### 2026-10-16 Arrange hover target after viewport movement

提交 `fb83d68` 修复 Main actions 40528/40529/40530 的 Arrangement hover target 在视图变化后失效：AAADAW 记录鼠标在 timeline 内容坐标中的位置，在滚轮/中键水平 pan、缩放、同步垂直滚动及轨道行布局重建后重新命中 item。新增两个回归验证水平 pan 后命中相邻 item、zoom 后进入空隙，以及垂直滚动后从一轨目标切换到另一轨目标。

`cargo test --quiet -p aaadaw --bin aaadaw` 通过 674/674；Core/App all-target Clippy `-D warnings`、`cargo fmt --all -- --check`、`git diff --check` 和 parity CSV 结构校验通过。重建桌面二进制 Build ID `3a3392dad282d4b91056360ba95627f8cf7d1e61` 在隔离 Xvfb :96 渲染实际 1280×800 窗口；启动图与已跟踪基线像素完全相同。该截图只是启动/render smoke，不是 hover 功能状态截图；功能行为由 App timeline regressions 验证。此处记录的 Action List 无目标启用与 range anchor 未验证状态已被下文 2026-10-16 原厂实测取代；selected-track set 与 last-touched-track 行为现已验证，primary active item 仍待确认，`ITEM-SELECT-006/007/008` 保持 `in_progress`。

### 2026-10-16 Main action 40528 empty-lane and Action List target behavior

REAPER 7.82 / Default 7 原生指针探针补齐 40528 的无目标语义：A1 上运行选择 A1，空白 item lane 上运行清空已有 item 选择；Action List 覆盖指针时 Run 仍可用，但运行 40528 不改变选择。40529 对 B1 重复执行不改变选择，40530 切换 B1 后保留 A1，而两项对空白 lane 都 no-op。原厂 7.82/Linux 与 Default 7 记录、每次指针坐标及窗口截图见 [item-hover native fixtures](../verification/reaper-parity/fixtures/item-hover-actions-native.tsv)；ReaScript 在 `scripts/reaper_parity/probe-item-hover-actions.lua`。

AAADAW 40528 现在保持可运行，在 track item lane 的空白处清空选择；没有有效 item lane target 时 no-op。空白清除通过应用级 item-selection history 可撤销，且不改变工程内容修订或 Project Undo 栈。真实桌面回归选中 Main 40528 后把鼠标停在第一条 MIDI item 并按 Enter，屏幕显示 item 高亮且 Action List Run 可用；当前二进制 Build ID `539e59063075287e32c116db4093aa10c664db38`、SHA-256 和 1280×800 截图记录在 [AAADAW fixture](../verification/reaper-parity/fixtures/item-hover-actions-aaadaw.tsv)。

定向回归 1/1、AAADAW 主测试套件 674/674、`cargo clippy -p aaadaw -p aaadaw-app --all-targets -- -D warnings`、fmt、diff 和 parity CSV（102 项、15 列、唯一 ID）校验通过。该次提交时的 40529/40530 anchor 与 Action List availability 未验证判断已由下文原厂探针更新；primary active item 仍需探测。

### 2026-10-16 Main actions 40529/40530：无目标时 Action List 仍可运行

REAPER 7.82 / Default 7 在零轨道、零 item 工程中分别选中 40529、40530 后，两项 Action List 的 Run 按钮都可用；点击后工程仍为空。原厂逐步记录和前后截图见 [availability TSV 与原厂截图](../verification/reaper-parity/fixtures/item-hover-actions-availability-native.tsv)。这修正了上文早期实现中“无 hover target 时禁用”的判断：AAADAW 现对 40528/40529/40530 始终报告可运行，由目标语义决定效果，无 target 时 40529/40530 no-op。

AAADAW 的 40529/40530 回归现在还检查 Action List 搜索结果始终 enabled，并在无 target 时 dispatch 后保留选择、修订号和 Project Undo/redo 状态。用当前桌面二进制在空工程中各运行一次，两项 Run 均保持可用且工程保持空；Build ID、binary SHA 与实窗截图记录在 [AAADAW availability fixture](../verification/reaper-parity/fixtures/item-hover-actions-aaadaw-availability.tsv)。`cargo test --quiet -p aaadaw --bin aaadaw` 674/674、严格 all-target Clippy、fmt 和 diff 检查均通过。此处记录的 Action List 无目标启用状态已完成；随后探针也确认了 40529/40530 的 range anchor 语义。`ITEM-SELECT-007/008` 仍 `in_progress`，因为 primary active item 和 Action List Run 与有效 hover target 的组合仍需覆盖。

### 2026-10-16 Main actions 40529/40530：保留 REAPER 的 range-selection anchor

为读出 REAPER 内部没有通过 ReaScript API 暴露的 range anchor，运行 [原厂探针](../scripts/reaper_parity/probe-item-hover-selection-anchor.lua) 后以真实鼠标输入做后续 Shift-click。使用全新 `-cfgfile` 配置，只复制 Default 7 主题，没有自定义 keymap、mouse map 或第三方扩展。基准中点击 A1 后 Shift-click B2 选择 A1/B1/B2；恢复 A1 后运行 40529 将 A2 加入，再对 B2 做相同 Shift-click 得到 A1/A2/B1/B2，说明动作保留原 A1 anchor。40530 加入 A2 后结果相同；先加入再将非锚点 A2 移除后仍得到 A1/B1/B2。单选 A2 后用 40530 移除唯一已选 item，下一次 Shift-click B2 得到 A2/B2，说明即使 anchor item 暂时不在选择集合中，原厂仍保留它。每个步骤的所选 IDs、action、指针坐标、REAPER project-state count 和 Undo label 见 [item-hover anchor TSV](../verification/reaper-parity/fixtures/item-hover-anchor-native.tsv)，原厂 Default 7 实窗截图记录基准选择与 40529 前后的高亮。

AAADAW 的 40529/40530 dispatch 现在保留现有 `selection_anchor`；`selected_item` 在新增目标时仍更新为 hovered item，移除当前 `selected_item` 时改为按 Arrangement 工程顺序选择首个剩余 item。回归检查覆盖 Add/Toggle 添加、移除非锚点、移除唯一锚点后的空选择、多个剩余 item 的确定性回退，以及 no-target no-op；还检查 project revision 和 Project Undo/redo 不变。原厂与 AAADAW 测试也确认选择集合保持 Track A、last-touched/primary track 转为 hovered Track B。随后新增的 MIDI Editor active-take 探针见下文；`ITEM-SELECT-007/008` 仍因 Action List Run 与有效 hover target 的组合及其他焦点边界未全覆盖而保持 `in_progress`。

本轮把 Track A 已选、指针悬停 Track B 的 B1 作为独立原厂场景：40529 添加 B1、40530 再将其移除，两次都保留 Track A 的 selected-track set，同时 `GetLastTouchedTrack()` 转到 B。API 顺序探针确认 `GetSelectedMediaItem()` 按工程顺序枚举；多选 Item Properties 没有显示独立 primary item。该段先记录了当时尚未完成的焦点探测，后续结论见下文。

### 2026-10-16 Main actions 40529/40530：MIDI Editor active take 与 toggle 回退顺序

为区分 Arrange 的选择集合与 MIDI Editor 的 active take，原厂探针给 A1/A2/B1/B2 分别写入音高 60/62/64/65，然后通过真实鼠标输入构造多选，再运行 Main action 40153 并读取 `MIDIEditor_GetTake()`。点击 A1 后 Ctrl-click B1（B1 最后点击）直接打开编辑器时，active take 是 A1/60；40529 在 A2 上添加后仍为 A1/60；先选择 A2、B1、最后点击 A1，再用 40530 移除 A1 后，active take 成为剩余选择中工程顺序最前的 A2/62。可见 active take 属于 MIDI Editor 上下文，且跟随当前所选 item 集合的工程顺序；它不能证明 Arrange 另有独立 primary active item。操作记录、API 返回和三张实际 Default 7 窗口截图见 [active MIDI take fixture](../verification/reaper-parity/fixtures/item-hover-active-midi-take-native.tsv)：[基准](../verification/reaper-parity/fixtures/item-hover-active-midi-take-baseline-native.png)、[40529](../verification/reaper-parity/fixtures/item-hover-active-midi-take-40529-native.png)、[40530](../verification/reaper-parity/fixtures/item-hover-active-midi-take-40530-native.png)。

AAADAW 原有 Toggle 在移除当前 `selected_item` 后用 HashSet 任取剩余项，可能偏离原厂所选集合的确定性次序。现在从 `TimelineCache.items`（track、start time、ID 顺序）取第一个剩余选中 item，`TimelineState::rebuild` 清理无效 primary 时也使用同一规则。新回归 `toggling_off_primary_item_chooses_first_remaining_item_in_project_order` 先以反序指定 A2/B1/A1，再从 Action List Toggle 移除 A1，断言回退到 A2。Focused regression 1/1、AAADAW 主测试 675/675、严格 all-target Clippy、fmt、diff、102×15 唯一 ID 矩阵和三张原厂截图 SHA-256 校验均通过。`ITEM-SELECT-007/008` 仍因有效 hover target 下的 Action List Run 组合及其它焦点边界未覆盖而保持 `in_progress`。

### 2026-10-16 Main actions 40529/40530：Audio item 有效 hover target

REAPER 7.82 / Default 7 原厂验证将一段 mono PCM16、48 kHz、1 秒 WAV 放入 Track B 的 Audio item B1，初始只选择 Track A 的 MIDI item A1。把原厂 Action List 放在 Arrangement 下方，真实鼠标停在 B1 并按 Enter 执行 40529 后，选择为 A1/B1；相同流程执行 40530 后恢复 A1。两次 `GetItemFromPoint()` 都返回 B1，last-touched track 为 B。逐步选择、API 顺序、project state count、Undo label 与环境信息见 [Audio hover fixture](../verification/reaper-parity/fixtures/item-hover-actionlist-target-audio-native.tsv)；WAV 测试源和 SHA-256 同列登记，[40530 运行前截图](../verification/reaper-parity/fixtures/item-hover-actionlist-target-audio-40530-before-native.png)展示选中的 B1 与 Action List 结果（鼠标坐标记录在 TSV），[运行后截图](../verification/reaper-parity/fixtures/item-hover-actionlist-target-audio-40530-after-native.png)展示 B1 已取消选择。

AAADAW 新回归 `hover_selection_actions_select_add_and_toggle_audio_items` 使用 Audio A1/B1 验证 Action List Add/Toggle 的 audio-item dispatch，同时断言先前选择、selected-track set、last-touched track 与工程 Undo/redo 状态。定向测试通过；`cargo test --quiet -p aaadaw --bin aaadaw` 676/676、`cargo clippy -p aaadaw -p aaadaw-app --all-targets -- -D warnings`、fmt、diff 与矩阵结构（102 行、15 列、唯一 ID）校验通过。`ITEM-SELECT-007/008` 仍保持 `in_progress`，因为更广的焦点边界和其它 item-selection 语义尚未全部验收。

### 2026-10-16 Main action 40528：Audio item 的有效 Action List target

REAPER 7.82 / Default 7 原厂探针确认 Main action 40528 可以从 Action List 键盘执行并命中 Audio item：初始选中 MIDI A1，将 `Item: Select item under mouse cursor` 搜索结果选中后，真实鼠标停在 Track B 的 Audio B1 并按 Enter，选择由 A1 替换为 B1。`GetItemFromPoint()` 记录 B1，last-touched track 为 B，工程状态计数增加且 Undo label 为 `Change media item selection`。Action List/Arrange 的前后实窗截图、API 选择顺序和环境详情见 [40528 Audio target fixture](../verification/reaper-parity/fixtures/item-select-actionlist-target-audio-native.tsv)、[运行前](../verification/reaper-parity/fixtures/item-select-actionlist-target-audio-40528-before-native.png)与[运行后](../verification/reaper-parity/fixtures/item-select-actionlist-target-audio-40528-after-native.png)。

AAADAW 回归 `hover_selection_actions_select_add_and_toggle_audio_items` 现覆盖同一 Audio A1/B1 fixture 上 40528 替换选择，以及 40529 添加和 40530 移除；随后两项仍验证既有 MIDI/Audio 选择、selected-track set、last-touched track 与项目 Undo/redo 不变。定向回归、App 主测试 676/676、严格 all-target Clippy、fmt、diff 与 parity 矩阵校验均通过；`ITEM-SELECT-006` 仍 `in_progress`，因为 40528 的其它无效焦点、空 lane 与边界状态组合尚未全验收。

### 2026-10-16 AAADAW Action List 40529 Audio hover screenshot

新增 AAADAW 桌面实窗验证：在 Action List 过滤并选中 Main 40529，将鼠标停在 Arrange 的 Audio B1 后按 Enter。B1 在运行前无选择边框，运行后显示橙色选择边框；屏幕同时包含独立 Actions 窗口和 Arrange。前后截图、1280×800 Xvfb 配置、Audio A1/B1 fixture、二进制 Build ID/SHA-256 与截图 SHA-256 见 [AAADAW TSV](../verification/reaper-parity/fixtures/item-hover-actionlist-audio-add-aaadaw.tsv)、[运行前](../verification/reaper-parity/fixtures/item-hover-actionlist-audio-add-aaadaw-before.png)和[运行后](../verification/reaper-parity/fixtures/item-hover-actionlist-audio-add-aaadaw-after.png)。项目生成器位于 `crates/aaadaw-storage/examples/generate_item_hover_audio_demo.rs`，使用已提交的 1 秒 PCM 测试 WAV 可重新创建相同素材布局。

这项实窗证据补齐了 AAADAW Action List 40529 在 Audio item 有效 hover target 上的截图；REAPER 原厂对照已覆盖 40529 添加及 40530 移除 Audio B1。当前环境中的 Xvfb/AAADAW 进程已结束，因此本次只新增 40529 的前后截图，40530 桌面有效目标仍需另行采集。测试生成器成功运行；截图对应二进制 Build ID `fda58cf28ba57c9c30f520bf78522b03ef32de2b`，SHA-256 `004e67d3bd3c4e5bea9efedf48bcb96a53a2b5f481f12ef8c3cccf789f21e8c5`。`ITEM-SELECT-007/008` 继续 `in_progress`，其它选择语义和焦点边界未全部验收。

### 2026-10-16 Main actions 41167–41174：保留 Audio item 的精确样本光标

修正最近 item 边缘导航和所选 item 起止光标的量化：Audio 边界现在以 `Sample` 保存，MIDI 边界仍以 `Tick` 保存；跨媒体比较统一到连续 sample 位置。Arrangement shader 接收分数 tick 并偏移光标，后续 Playback seek、Audio Loop paste、光标处分割、导入定位、Nudge 与工程起点使用同一精确样本。普通 tick 光标事件会清除精确样本覆盖。

`item_edge_navigation_actions_move_to_nearest_audio_or_midi_boundary` 和 `selected_audio_item_cursor_actions_preserve_sample_exact_boundaries` 覆盖非 tick 对齐边界、左右导航、41173/41174、普通 tick 重置、导入、Loop paste 与 Audio 分割。REAPER 7.82 对真实 PCM take 测得 41167/41168/41173/41174 在 Snap on/off 下都精确返回 sample 192012 或 240012，且 project state count 不变；原厂 full-precision TSV 与 Arrange 截图见 [native cursor fixture](../verification/reaper-parity/fixtures/item-cursor-sample-native.tsv)。Linux AAADAW 实窗中选中同一起点的 Track B Audio item，并在 Action List 运行 Main 41173；Transport 到达 4 秒、Inspector 仍显示 sample `192012`，Xorg :112 / 1280×800 截图及 Build ID、binary hashes 见 [AAADAW cursor fixture](../verification/reaper-parity/fixtures/item-cursor-sample-aaadaw.tsv)。项目生成器现可传入 off-grid 起始 sample。

AAADAW 主测试目标 678/678 passed；`cargo clippy --locked -p aaadaw -p aaadaw-app --all-targets -- -D warnings`、fmt 与 `git diff --check` 通过。截图只能证明真实 Action List 操作与当前界面状态；当前最大缩放下子 tick 的屏幕位移小于一像素。41174 独立 AAADAW 实窗截图、Action List 空选择可用性和完整焦点/界面验收仍待补证，`ITEM-SELECT-012/013/016/017` 保持 `in_progress`。

### 2026-10-16 AAADAW Main action 41174 与空选择 Action List 实窗

使用与 41173 相同的 off-grid Audio template、Build ID `501a2645dcc0be1bc0b878cef77851f433e9afe8`，在 Xorg :112 / 1280×800 中实际打开 Main Action List 并运行 41174。Transport 到达 `0:05.000`，Audio item 仍选中，Inspector 仍报告起始 sample `192012`；Action List Run 后结果保持选中，滚动 Arrange 后可见 item 末端。运行前后截图和 hashes 见 [41174 AAADAW fixture](../verification/reaper-parity/fixtures/item-cursor-end-aaadaw.tsv)。

随后清空 item 选择并在 Transport `0:06.000` 时从 Action List 再运行 41174：结果仍可选，Run 按钮仍启用，运行前后 Transport 保持 `0:06.000`、Inspector 显示无选择项。这补齐 AAADAW 侧空选择可用状态和 no-op 的真实窗口证据；REAPER 原厂 no-selection no-op 仍见 [item navigation fixture](../verification/reaper-parity/fixtures/item-navigation-native.tsv)，其 Action List 按钮状态尚无独立窗口截图。截图、Build ID、binary SHA 和操作步骤见 [empty-selection AAADAW fixture](../verification/reaper-parity/fixtures/item-cursor-empty-selection-aaadaw.tsv)。完整 Action List 焦点和整体 Arrange UI parity 仍开放，`ITEM-SELECT-016/017` 保持 `in_progress`。


### 2026-10-17 Audio Take playback rate：模型、Properties、播放与 Copy loop

按 Linux REAPER 7.82 / Default 7 原厂属性探针实现 Audio Take playback rate。Audio item 默认 1.0；Core action 可验证、撤销和重做，snapshot 与 SQLite schema 57 持久化，旧 schema 56 项目迁移为 1.0。Audio Item Properties 增加 Take rate 文本字段，并与其他改动作为一次 Apply/Undo 事务；速率改动要求先关闭音频输出，避免打开的 playback graph 使用旧 feeder。所有 Audio item 文件/reader realtime 与 offline feeds 现在将 Take rate 交给非 realtime 解码/重采样 worker。

Copy loop clipboard 保留 Take rate。原厂探针的 1.25x 示例在 AAADAW 回归中验证：3–7 秒时间选区仍粘贴为 4 秒，但源区段位置和长度按 1.25x 映射为 3.75–8.75 秒；Paste、Undo、Redo 保留速率。媒体端到端回归以 1 kHz WAV 验证 1.25x feeder 输出约 1.25 kHz。最新 Cranelift 二进制在 Linux Xvfb 中实际打开 Audio Item Properties，窗口显示加载的 1.25 Take rate；[截图、窗口大小和精确二进制 hashes](../verification/reaper-parity/fixtures/audio-take-rate-properties-aaadaw.tsv)。Apply/Undo/Redo 与音频输出活动时拒绝速率修改另由 App 回归验证。属性窗从 526×500 调整为 526×526，截图确认 Apply 按钮完整可见。这张图只验 AAADAW 界面，不是 REAPER 原厂对照。

Cranelift 1.101 workspace Nextest 全量 1306/1306、0 skipped 通过，X11 测试运行于 Xvfb :335；Stable 五 crate audio-device all-target Clippy `-D warnings`、fmt 和 diff 检查通过。两个 parity 条目保持 `in_progress`：Preserve pitch 模式、原厂速率范围、完整媒体边界组合和 UI 原厂窗口对照仍未覆盖；有效输入采样率的整数 Hz 量化已由下文工作修复。还没有新增 REAPER 原厂截图，也没有把属性窗口或 Copy loop 目标标记为完成。

### 2026-10-17 Audio Take rate：保留小数采样率

修复 Take rate feeder 将有效输入采样率四舍五入到整 Hz 的偏差。解码 worker、循环源帧映射与流式 sinc 重采样器现在用整数微 Hz 定点值表示速率，避免浮点相位累积，同时让小数 Take rate 参与滤波 cutoff、插值相位和持续时间。普通整数速率的公开重采样 API 仍以 Hz 接受参数。

新增 `take_rate_resampling_preserves_fractional_hertz_and_duration`：48 kHz、1.00001 倍速保持 48,000.48 Hz；1.0001 倍速下对 480,000 个源帧实测输出帧数与旧整 Hz 量化算法不同，常量信号幅值保持。Cranelift 定向回归通过。`ITEM-PROPERTIES-001` 与 `ITEM-AUDIO-COPYLOOP-001` 仍为 `in_progress`：REAPER Preserve pitch、原厂字段/窗口比较及更广的 loop 边界组合仍未覆盖。

### 2026-10-17 Action List 快捷键：Named key 往返

扩展 Action List 快捷键捕获与配置解析，支持 iced 0.14 暴露的全部 295 个非修饰 `Named` 键：F1–F35、导航与编辑键、浏览器/媒体/音量、启动和输入法键等。Alt、AltGraph、Control、Fn、FnLock、Shift、Symbol、SymbolLock、Meta、Hyper、Super 仍作为修饰键处理，不能单独成为组合主键。逐键回归以 Ctrl+Alt+Shift+Super 验证实际捕获、精确匹配、标签序列化和配置解析；同时把原 `F25` 负例更新为 `F36`。

Cranelift 1.101 AAADAW binary tests 694/694 passed；新增逐键测试 1/1 passed；Stable `cargo clippy -p aaadaw --bin aaadaw --all-targets -- -D warnings`、fmt 和 diff 检查通过。Xvfb 实窗中从 Actions 为 Edit: Undo 打开 Keyboard input 并输入 F35，窗口显示录入草稿；[截图 fixture](../verification/reaper-parity/fixtures/action-list-named-key-aaadaw.tsv) 记录窗口大小、Build ID、二进制和截图哈希。这只证明 AAADAW 捕获可用，不是 REAPER 对照；本轮未新增原厂探针。REAPER Special keys 对话框、Scope、MIDI/OSC、Action Section 和重复绑定完整优先级仍未对齐，`INPUT-BIND-001` 继续 `in_progress`。

### 2026-10-17 Action List Special key mode：Enter/Tab 绑定录入

Linux REAPER 7.82 / Default 7 的 387×229 Keyboard/MIDI/OSC Input 实窗确认：点击 `Special key (Enter, Tab, etc.)` 后提示 `<type special key now>>`，按 Enter 将字段设为 `Enter` 且窗口仍打开；Cancel 丢弃草稿。AAADAW 加入同名入口和独立待捕获状态。Xvfb 实窗中该模式提示与 REAPER 一致，按 Enter 显示 `Enter`，随后点击 OK 将其保存为 Edit: Undo 的附加绑定 `Mod+Z; StandardEnter`，Action List 窗口显示 `Shortcut saved`。配置保留 `StandardEnter` 的主键区身份，界面显示与原厂一致的 `Enter`。捕获期间 Enter 不再误作确认键。REAPER 原厂提示/Enter 截图、AAADAW 同尺寸输入窗两张截图、保存结果、二进制 Build ID 与 SHA-256 均见 [special-key fixture](../verification/reaper-parity/fixtures/keyboard-special-key-aaadaw.tsv)。

`special_key_capture_records_enter_and_tab_without_confirming_the_binding` 与 `shortcut_labels_use_the_platform_modifier_name` 两项 Stable 定向回归通过；Stable aaadaw all-target Clippy `-D warnings` 通过。更新二进制 Build ID、SHA-256、native/AAADAW 实窗截图哈希见 fixture。Cranelift workspace nextest 冷链接在环境重启时被终止，未取得结果，不作为通过记录。矩阵只记录本次特殊键输入路径，`INPUT-BIND-001` 仍为 `in_progress`：Scope、MIDI/OSC、自动关闭、Section/跨 Section 优先级、重复绑定全规则及完整窗口仍待实现与验收。

### 2026-10-17 File > Project templates: native cascade order and empty state

REAPER 7.82 / Default 7 was rerun under an isolated configuration with zero, one, and two `.RPP` templates. Captures now establish its exact cascade order: `Save project as template...`, `Show project template path in explorer/finder`, separator, disabled `Load project template:` header, then case-insensitively sorted names without `.RPP`; the empty state reads `(no project templates)`. The native save dialog opens in `ProjectTemplates`, and an existing file triggers an `Overwrite?` confirmation. Cropped captures and hashes are in [native fixture TSV](../verification/reaper-parity/fixtures/project-template-menu-native.tsv).

AAADAW now uses the same menu order and disabled header/empty-state structure. The new folder action opens `ProjectTemplates` through `xdg-open` on Linux, `open` on macOS, and `explorer` on Windows. Pointer and keyboard navigation skip the disabled header and empty-state row; the populated cascade test confirms Save, folder action, then template navigation. An isolated Linux desktop run of the newly built binary captured both populated and empty cascades; XTest invoked the folder action and verified the exact path argument and status-bar result. Full-screen screenshots, menu crops, template file hashes, binary SHA-256 and Build ID are in [AAADAW fixture TSV](../verification/reaper-parity/fixtures/project-template-menu-aaadaw.tsv).

Stable `cargo xtest -p aaadaw --bin aaadaw` passed 696/696; the focused template selection ran 8/8 and the save-without-overwrite case passed 1/1. Strict all-target Clippy with `-D warnings`, rustfmt, `git diff --check`, parity matrix shape/unique-ID checks, and all referenced evidence paths passed. The local container lacked GLib pkg-config development files, so build checks used ignored `target/pkgconfig` entries pointing at its installed GLib 2.84 runtime libraries; no dependency or source configuration changed for that workaround.

`SHELL-PROJECT-TEMPLATE-001` remains `in_progress`: AAADAW deliberately preserves the existing template when a duplicate name is selected but does not yet show REAPER's overwrite confirmation; Android saving, multiple project tabs, and the full REAPER template/default-project format remain incomplete.

### 2026-10-17 MIDI export: 24 fps SMPTE offset byte parity

An isolated Linux REAPER 7.82 / Default 7 probe loaded a project at 24 fps non-drop with a 10.5-second project offset. The native Type 1, PPQN 960 export with Embed SMPTE offset checked contains `FF 54 05 00 00 0A 0C 00` (00:00:10:12); the complete 107-byte file now matches AAADAW byte-for-byte. The probe reads the frame rate and project offset back from the reopened project, and the native export dialog capture confirms Type 1, PPQN 960, and the checked SMPTE option. Project, MIDI, screenshot hashes and probe results are in the [24 fps SMPTE fixture](../verification/reaper-parity/fixtures/menu-export-midi-native-smpte-24fps-nonzero-offset.tsv).

Added `reaper_782_type_one_24fps_nonzero_smpte_offset_matches_native_bytes`. Stable `aaadaw-core` Nextest passed 159/159; strict all-target Core Clippy with `-D warnings`, rustfmt, and `git diff --check` passed. `FILE-MIDI-EXPORT-001` remains `in_progress`: 23.976/25/29.97 fps and drop-frame offsets, remaining option combinations, and full dialog alignment still need native comparison.

### 2026-10-17 MIDI export: 25 fps SMPTE offset byte parity

Extended the isolated Linux REAPER 7.82 / Default 7 probe to load a 25 fps non-drop project with a 10.5-second offset and verify both values through REAPER's API. The native Type 1, PPQN 960 export with Embed SMPTE offset checked contains `FF 54 05 20 00 0A 0C 00` (00:00:10:12); all 107 output bytes match AAADAW. The checked native export dialog, project, MIDI file, binary identity and hashes are recorded in the [25 fps fixture](../verification/reaper-parity/fixtures/menu-export-midi-native-smpte-25fps-nonzero-offset.tsv).

Added `reaper_782_type_one_25fps_nonzero_smpte_offset_matches_native_bytes`. Stable `aaadaw-core` Nextest passed 160/160; strict all-target Core Clippy with `-D warnings`, rustfmt, and `git -c core.whitespace=cr-at-eol diff --check` passed (the native RPP fixture retains REAPER's CRLF line endings). `FILE-MIDI-EXPORT-001` remains `in_progress`: 23.976/29.97 fps, drop-frame offsets, remaining option combinations, and full dialog alignment still need native comparison.

### 2026-10-17 MIDI export: 23.976 fps SMPTE offset byte parity

In Linux REAPER 7.82 / Default 7, Project Settings > Video was set to the native 23.976 preset and Project Settings shows a 10.5-second start offset. After reopening the saved project, REAPER's API reports 23.976023976024 fps, non-drop, and exactly 10.500000000 seconds. The Type 1 / PPQN 960 export with `Embed SMPTE offset` checked contains `FF 54 05 00 00 0A 0B 00` (00:00:10:11); AAADAW now matches the complete 110-byte native file. The checked export dialog, project, MIDI, both Project Settings pages, binary identity and hashes are recorded in the [23.976 fps fixture](../verification/reaper-parity/fixtures/menu-export-midi-native-smpte-23976-nonzero-offset.tsv).

Extended the reusable SMPTE-rate probe to accept 23.976, 24, and 25 fps; a second isolated REAPER run generated the 23.976 project and verified the saved RPP/API readback. Added `reaper_782_type_one_23976fps_nonzero_smpte_offset_matches_native_bytes`. Full `aaadaw-core` Nextest passed 161/161, strict all-target Core Clippy with `-D warnings`, and rustfmt check passed. `FILE-MIDI-EXPORT-001` remains `in_progress`: 29.97 fps/drop-frame offsets, other export combinations, and full dialog alignment still need native comparison.

### 2026-10-17 MIDI export: 29.97 drop-frame SMPTE offset byte parity

REAPER 7.82 / Default 7 Project Settings > Video was set to the native `29.97DF` preset. After saving and reopening, REAPER reports 29.970029970030 fps, drop-frame enabled, and a 10.5-second project offset; the saved RPP uses `TIMEMODE 1 3 -1 30 1 0 -1 0`. The Type 1 / PPQN 960 export with `Embed SMPTE offset` checked contains `FF 54 05 40 00 0A 0E 00` (00:00:10:14); the complete 110-byte file matches AAADAW. The project settings, checked export dialog, MIDI and project hashes are in the [29.97DF fixture](../verification/reaper-parity/fixtures/menu-export-midi-native-smpte-2997drop-nonzero-offset.tsv).

Extended the repeatable rate probe for `29.97DF`, including its saved time-mode encoding and drop-frame API readback, and added `reaper_782_type_one_2997drop_nonzero_smpte_offset_matches_native_bytes`. The focused comparison passes, and the reusable REAPER probe also generated a project whose API readback and saved RPP match the native UI capture. Full `aaadaw-core` Nextest passed 162/162, strict all-target Core Clippy with `-D warnings`, rustfmt check, and CRLF-aware diff checks passed. `FILE-MIDI-EXPORT-001` remains `in_progress`: other 29.97/drop-frame offsets, remaining export combinations, and full dialog alignment still need native comparison.

### 2026-10-17 MIDI export: REAPER 29.97 non-drop SMPTE mapping

The native `29.97ND` Project Settings preset reads back as 29.970029970030 fps with drop-frame disabled. Despite the SMF format lacking a 29.97 non-drop rate code, REAPER exports the enabled SMPTE offset using the 30 fps code (`0x60`) and its 29.97 frame-floor result: at 10.5 seconds the event is `FF 54 05 60 00 0A 0E 00` (00:00:10:14). The complete native Type 1 / PPQN 960 file is 108 bytes and the [29.97ND fixture](../verification/reaper-parity/fixtures/menu-export-midi-native-smpte-2997nd-nonzero-offset.tsv) records the RPP/API readback and checked-dialog evidence.

Changed AAADAW's SMPTE encoder to map `Fps2997` to the native 30 fps SMF rate code while retaining frame numbering from the 29.97 project rate. Replaced the rejection-only unit regression with a native-code expectation and added `reaper_782_type_one_2997nd_nonzero_smpte_offset_matches_native_bytes`; both focused 29.97 tests pass. The reusable REAPER rate probe now covers `29.97DF` and `29.97ND`, and a fresh REAPER run verifies its generated ND project/API readback. Full `aaadaw-core` Nextest passed 163/163, strict all-target Core Clippy with `-D warnings`, rustfmt check, and CRLF-aware diff and fixture-reference checks passed. `FILE-MIDI-EXPORT-001` remains `in_progress` for more SMPTE offsets/rates, other export combinations, and full dialog alignment.

### 2026-10-17 MIDI export: 29.97 drop-frame minute-boundary parity

REAPER 7.82 / Default 7 was probed at a 60.060-second project offset, where its Project Settings displays `1:00.060` and the 29.97DF SMPTE counter crosses the first dropped-minute labels. The native Type 1 / PPQN 960 export writes `FF 54 05 40 01 00 02 00` (`00:01:00:02`) and all 110 bytes match AAADAW. The saved project, start-time and 29.97DF settings screenshots, checked dialog, MIDI hash and REAPER identity are in the [minute-boundary fixture](../verification/reaper-parity/fixtures/menu-export-midi-native-smpte-2997drop-minute-boundary.tsv).

Added `reaper_782_type_one_2997drop_first_minute_boundary_matches_native_bytes` plus a timecode regression that formats and parses the `60.06` / `0:01:00:02` boundary. Both focused tests pass. The shared SMPTE probe now accepts a requested project offset and computes the expected event from the selected rate, offset, and drop-frame rules. Full `aaadaw-core` Nextest passed 165/165, strict all-target Core Clippy with `-D warnings`, rustfmt check, and CRLF-aware diff checks passed. `FILE-MIDI-EXPORT-001` remains `in_progress` for additional offsets/rates, other export combinations, and full dialog alignment.

### 2026-10-17 Action List shortcut-map import/export

AAADAW Action List `Options` now opens import/export entries for a versioned `AAADAW_SHORTCUT_MAP` file. The portable file contains stable AAADAW action IDs and canonical shortcut alternatives, preserving explicit empty overrides; importing replaces only the user override layer and returns omitted actions to factory bindings. Import validates version, size, rows, shortcut syntax, IDs, and macro conflicts before one atomic write, so invalid maps or save failures leave active shortcuts intact. Export uses atomic replacement and refuses to overwrite the application’s active `shortcuts.conf`.

The four format/read/write tests and two app-level transactional import tests pass; all 704 AAADAW desktop tests pass. Stable `cargo check`, build, formatting and diff checks pass. A real 701×475 Action List/Xvfb screenshot of the expanded Options menu is recorded in the [fixture](../verification/reaper-parity/fixtures/action-list-keymap-import-export-aaadaw.tsv). Native chooser automation could not run in that minimal Xvfb image because neither `xdg-desktop-portal` nor `zenity` was installed; the file reader/writer and application state replacement were tested directly. The AAADAW-native format remains the portable backup for the full AAADAW override set; limited REAPER `.ReaperKeyMap` support is documented below.

### 2026-10-17 REAPER `.ReaperKeyMap` Main-section subset

An isolated Linux REAPER 7.82 / Default 7 UI probe exported and re-imported native keymaps. Native export confirms `KEY <flags> <Windows virtual-key> <action ID> <section ID>`; the fourth value is the Section ID (`0` for Main), distinct from the Keyboard Scope control. Observed modifier flags are base `1`, Shift `4`, Ctrl `8`, Alt `16`, and Logo `32`; F11/F12 virtual-key codes are `122`/`123`, Z is `90`, and action `40029` is Edit: Undo. The exact native-exported rows, AAADAW-format import row, both native screenshots, REAPER binary version/hash and screenshot hashes are in [the native keymap fixture](../verification/reaper-parity/fixtures/reaper-keymap-native.tsv).

The Action List now provides distinct REAPER Main-map import/export entries. The codec maps 102 verified REAPER 7.82 Main actions, handles the observed modifier bits and supported Windows virtual keys, and imports/exports only explicit user overrides. Import rejects non-Main Section IDs, unknown action IDs, `SCR` and other record types, invalid flags and unsupported key codes before changing active settings; export rejects unmapped actions, empty overrides and keys that do not round-trip. A native REAPER import of `KEY 17 122 40029 0` visibly added Alt+F11 to Edit: Undo while retaining factory Ctrl+Z.

Stable `cargo check` and all 712 AAADAW desktop tests pass; the 17 filtered `reaper_` tests cover modifier round trips, the native export sample, unsupported-record rejection and protection of active shortcut configuration. REAPER native import behavior is confirmed in the live UI. AAADAW's rebuilt desktop binary (Build ID `0da735bc2e079a7576bec5d43262482dd9121427`, SHA-256 `6b6d023c3e4ecca294d08140776920276fb73cb6c6486867054a5b94321de446`) opened the standalone 701×475 Action List on Xvfb :828; its Options menu visibly contains all four AAADAW and REAPER Main import/export entries. Screenshot SHA-256 and capture steps are in the [AAADAW Action List fixture](../verification/reaper-parity/fixtures/action-list-keymap-import-export-aaadaw.tsv). The 102-action map now also covers file/render commands, item editing and navigation, loop/time-selection controls, project markers and regions, folder/track controls, and native view actions. Every mapped REAPER ID/name matches Section 0 of the captured catalog and an exhaustive codec test round-trips the unique ID table. Native REAPER accepted a four-action editing map; its Nudge/Set row shows imported F9 while retaining factory N. The `.ReaperKeyMap` sample and screenshot are recorded in the native fixture. Bundled German/DK `action_id=0` keyboard-layout remaps, scripts/custom actions, the full native action catalog, other Sections, Scope and MIDI/OSC remain open. `INPUT-BIND-001` remains `in_progress`.

### 2026-10-17 File > Project templates: confirmed atomic overwrite

Saving over an existing `.aaadaw` template now opens a modal confirmation with its absolute path and `Overwrite?`; OK rechecks the destination and replaces it atomically from a temporary SQLite project, while Cancel, Escape, and backdrop dismissal preserve the existing bytes. The command refuses to replace a non-file, a disappeared destination, or the currently open project template. A failed replacement leaves the prior file readable. New destination saves still use no-clobber semantics. The save request flows through the real selection helper; the application test verifies the modal blocks background project edits until Cancel. Four regressions cover a successful replacement, write-failure preservation, modal cancellation, and the open-project guard.

Stable AAADAW desktop Nextest passed 716/716. Strict all-target Clippy with `-D warnings`, rustfmt, and `git diff --check` passed. A live AAADAW 1280×800 Xvfb screenshot and 520×122 crop are recorded with the final production binary identity in [the overwrite fixture](../verification/reaper-parity/fixtures/project-template-overwrite-aaadaw.tsv). The capture-only modal seed was temporary and is absent from source and the final production binary. That production binary (Build ID `15f1879ec05fe16f99974e024d517cb789f6943b`, SHA-256 `fb584c8e12d4a32898c559f9fb8d4c163523934067786122dac17057b0259146`) also launched normally on Xvfb :945 with a visible 1280×800 main window. The environment had no XDG desktop portal or `zenity`, so the normal native picker was not available to drive the whole flow in the desktop session; app-level tests cover the resulting path selection and modal logic. AAADAW uses a dark surface, blue primary action, and different text spacing from the native 521×126 light-gray prompt, so visual styling remains open. Android saving, multiple project tabs, and the complete REAPER template/default-project format also remain open; `SHELL-PROJECT-TEMPLATE-001` stays `in_progress`.


### 2026-10-17 Main menu Home/End command selection

Linux REAPER 7.82 / Default 7 was driven with actual X11 keyboard input: with File open, Home highlights `New project` and End highlights `Quit`; both leave the menu open and execute nothing. Native screenshots, selection, stopped transport, and hashes are recorded in [the REAPER boundary fixture](../verification/reaper-parity/fixtures/menu-keyboard-boundary-native.tsv).

AAADAW now handles Home/End as first/last enabled-command navigation in main menus and the Recent projects/Project templates cascades. Keyboard events already consumed by another control stay with that control. Regressions check event routing (including captured-focus behavior), no project revision during boundary navigation, and first/last selection in both cascades. The live 1280×800 Xvfb replay first activated the window through XTest mouse input under xfwm4, then sent Alt+F, Home, End, and Escape; the empty project stayed open, with no command activation. The selected rows and screenshot hashes are in [the AAADAW fixture](../verification/reaper-parity/fixtures/menu-keyboard-home-end-aaadaw.tsv).

Stable AAADAW desktop Nextest passed 716/716. Strict all-target Clippy with `-D warnings`, rustfmt, `git diff --check`, and the 102-row/15-column unique-ID matrix check passed. Build ID `09e885366201fc507579e5719364829a8dd01a5e`, binary SHA-256 `ac9eb46e13196704806aaad49c782ba8ce612267cc22b7ac1b807aab0b644732`. `SHELL-MENU-KEYBOARD-001` stays `in_progress`: mouse/touch focus, text-control exception cases, full menu contents/order, and visual alignment are still open.

### 2026-10-17 Type 0 MIDI export with a hidden marker ruler

Linux REAPER 7.82 / Default 7 was driven through the actual Export Project MIDI dialog using the saved `RULERLANECFG 4` project created by Main action 43485. All four Type 0 states were exported to unique files outside the project's active MIDI source path. With `Export hidden markers` off, marker and cue modes omit both ruler-hidden events (45 bytes each); with it on, marker mode writes `#Intro` and `Verse` as `FF 06`, while cue mode writes the same labels as `FF 07` (64 bytes each). Dialog captures, hashes, native files, project reference, and settings are recorded in [the Type 0 ruler-hidden manifest](../verification/reaper-parity/fixtures/menu-export-midi-native-type0-marker-ruler-hidden.tsv).

The encoder now treats ruler-hidden and individually hidden markers separately: Type 0 honors the checkbox for ruler-hidden events but continues to omit individually hidden events, matching the distinct native cases. Four full-file regressions cover the marker/cue checked and unchecked combinations; the existing Type 0 individual-visibility regressions remain in place. Stable `aaadaw-core` Nextest passed 169/169, strict all-target Core Clippy passed, and rustfmt, CRLF-aware diff, fixture hashes, and the 102-row/15-column parity matrix checks passed. `FILE-MIDI-EXPORT-001` remains `in_progress` for untested export combinations, exact dialog styling, and the broader File menu.

### 2026-10-17 Type 0 ruler-hidden and individually hidden markers

A second Linux REAPER 7.82 / Default 7 project combined `RULERLANECFG 4` with the individually hidden `#Hidden` marker (flags 24). Its REAPER readback confirmed `#Intro` at tick 0, `Verse` at tick 960, and `#Hidden` at tick 1920. With Type 0 and `Export hidden markers` checked, marker mode includes only `#Intro` and `Verse` (`FF 06`); cue mode does the same with `FF 07`. Both complete 64-byte files, the saved project, project readback, and checked-state dialogs are in [the intersection fixture manifest](../verification/reaper-parity/fixtures/menu-export-midi-native-type0-marker-ruler-individual-hidden.tsv). Two new full-file regressions confirm that enabling ruler-hidden inclusion does not expose individually hidden markers or cues in Type 0.

### 2026-10-17 Type 0 hidden-ruler name filtering

With that combined project loaded in REAPER, Type 0 `Export hidden markers` and the `#` name filter were both checked. Marker mode retained only the `#Intro` `FF 06` event; cue mode retained only the `#Intro` `FF 07` event. Both 55-byte files match the established Type 0 filtered outputs exactly, excluding `Verse` by name and individually hidden `#Hidden` by visibility. Their independent dialog screenshots, native files, hashes, and settings are in [the filtered intersection manifest](../verification/reaper-parity/fixtures/menu-export-midi-native-type0-marker-ruler-individual-hidden-filtered.tsv). Two full-file tests cover marker and cue modes.

### 2026-10-17 SMPTE 29.97DF hour boundary

The native rate probe created and reopened a Linux REAPER 7.82 / Default 7 project at 29.97DF with a 3600-second project offset. The API readback confirmed the rate, drop-frame flag, and offset; Project Settings displays `1:00:00.000`. REAPER's Type 1 export writes `FF 54 05 41 00 00 00 00` (`01:00:00:00`) and the full 129-byte file matches AAADAW. Project/readback settings, screenshots, export, and hashes are in [the hour-boundary manifest](../verification/reaper-parity/fixtures/menu-export-midi-native-smpte-2997drop-hour-boundary.tsv). The new full-file regression passed; stable Core Nextest passed 174/174 and strict all-target Core Clippy passed.

### 2026-10-17 MIDI track-name encoding and VLQ length

Three saved Linux REAPER 7.82 / Default 7 projects extend Type 1 export coverage. `轨` plus 125 ASCII `A` characters is retained in the project as 128 UTF-8 bytes; REAPER exports the single non-ASCII scalar as `_`, producing a 126-byte MIDI track-name field. ASCII names of 127 and 128 bytes use VLQ lengths `7F` and `81 00` respectively. Native MIDI files, projects, unchecked-option dialog captures, and SHA-256 manifests are in the [Unicode-name evidence](../verification/reaper-parity/fixtures/menu-export-midi-native-track-name-unicode.tsv), [127-byte ASCII evidence](../verification/reaper-parity/fixtures/menu-export-midi-native-track-name-127-ascii.tsv), and [128-byte ASCII evidence](../verification/reaper-parity/fixtures/menu-export-midi-native-track-name-long-ascii.tsv). AAADAW now applies the observed replacement and all three full-file tests pass; focused Nextest passed 3/3, full Core Nextest passed 177/177, strict all-target Core Clippy passed, and rustfmt/diff checks are clean.

### 2026-10-17 MIDI export consolidation scopes

The native REAPER export dialog comparison exposed missing Input controls. A saved project with one selected track, one selected item on another track, and a 1–2 second time selection shows that All exports both tracks and all three items, Selected tracks emits Track B only, Selected items emits the selected C4 on Track A, and Time selection only emits the overlapping C#4 with its project tick 2400 rebased to output tick 480. A boundary probe confirms that time selection clips an overlapping note's start to zero, retains a note-off only when it falls before the selection end, and excludes events beginning at the end. AAADAW now carries selected track/item IDs and the time range through its worker snapshot, shows all native radio choices, and encodes each scope. Five core tests compare complete files, and the missing-selection error has a regression. Native projects, all dialog states, MIDI files, screenshots and hashes are in the [scope manifest](../verification/reaper-parity/fixtures/menu-export-midi-native-consolidation.tsv) and [boundary manifest](../verification/reaper-parity/fixtures/menu-export-midi-native-time-selection-boundaries.tsv). Core Nextest passed 183/183; the AAADAW binary suite passed 716/716; strict all-target Clippy, rustfmt, and CRLF-aware diff checks passed. The rebuilt binary opened File > Export project MIDI under Xvfb; the 520×452 live dialog and interactive Time selection only radio state are recorded in [the AAADAW scope manifest](../verification/reaper-parity/fixtures/menu-export-midi-aaadaw-scopes.tsv).

### 2026-10-17 MIDI export empty scopes and mute handling

REAPER 7.82 / Default 7 probes confirm that an empty project, a project with only a muted MIDI track, and a project with only a muted MIDI item all display `No unmuted tracks containing MIDI items were found. Write MIDI file header anyway?`. Selected Tracks and Selected Items use distinct warnings, each 377×81 for Entire project and 521×81 for Time selection only. Confirming writes a Type 1 / PPQN 960 file with the conductor and one empty MIDI track. Scope intersections also match: out-of-range selected MIDI items produce the longer native warning and empty-track file; a selected track whose only MIDI item lies outside the time selection, and All + Time Selection with its item before the range, each produce their scope-specific warning and the same 68-byte, two-track header. Overlapping selected-track or selected-item scopes write only the in-range C#4 rebased to tick 480. The native files, projects, warning/dialog captures, REAPER build identity, and hashes are in the [scope intersection fixture](../verification/reaper-parity/fixtures/menu-export-midi-native-scope-intersections.tsv).

AAADAW scope filtering now excludes muted MIDI items and MIDI items on tracks whose own mute flag is set before both warning detection and encoding. REAPER 7.82 does not propagate a muted folder's state into MIDI project export: an unmuted child MIDI track still exports, confirmed against a saved native project and byte-for-byte output comparison. The [folder-mute fixture](../verification/reaper-parity/fixtures/menu-export-midi-native-folder-muted.tsv) records this behavior. The header-only confirmation has the native message, OK/Cancel controls, gray surface, and native line wrap. Actual-window captures verify 324×81 for All/Entire project and 377×81 with the scope-specific wording for Selected Tracks and Selected Items; Cancel creates no file, and OK writes a valid two-track Type 1 file. The [All, Selected Tracks, and Selected Items prompt captures](../verification/reaper-parity/fixtures/menu-export-midi-native-scope-intersections.tsv), [cancel/confirm screenshots](../verification/reaper-parity/fixtures/menu-export-midi-aaadaw-empty-export-cancelled.png), [output file](../verification/reaper-parity/fixtures/menu-export-midi-aaadaw-empty-export.mid), and hashes are included in that fixture manifest. Native mute and empty-scope probes are reusable from the [mute probe](../../scripts/reaper_parity/probe-midi-export-muted-items.lua), [folder-mute probe](../../scripts/reaper_parity/probe-midi-export-muted-folder.lua), and [empty-scope probe](../../scripts/reaper_parity/probe-midi-export-empty-scopes.lua).

Full-file Core regressions compare muted-track, muted-item, muted-folder-with-unmuted-child, All + Time Selection with no overlap, and Selected Tracks + Time Selection with no overlap, alongside empty-project and other tested scope intersections. Combined Nextest passed 932/932 across 45 binaries (AAADAW 740, Core 192); strict all-target Clippy with `-D warnings`, rustfmt, CRLF-aware diff checks, and the production desktop build passed. The verified binary is Build ID `76c5a13da11f391ea5e6c69c459b3a3381c58136`, SHA-256 `7f41a1e165072622023b13ffe77604f5f19ec09619919130705602150a67cebb`. `FILE-MIDI-EXPORT-001` remains `in_progress`: further scope/metadata intersections, keyboard/focus behavior, and full visual alignment remain open.

### 2026-10-17 empty Type 0 MIDI export failure behavior

A fresh REAPER 7.82 / Default 7 probe confirms that Type 0 `Merge to single MIDI track` with `All / Entire project` and no qualifying MIDI items displays a single-OK `MIDI export failed!` dialog and writes no file. AAADAW's empty-scope preflight now distinguishes Type 0 from Type 1: Type 0 reports the native failure and never starts encoding; Type 1 retains its OK/Cancel prompt and header-only output. Confirm and Escape dismiss the Type 0 error as a failure. Native project, probe script, screenshots, hashes, and no-output evidence are in [the Type 0 fixture manifest](../verification/reaper-parity/fixtures/menu-export-midi-native-empty-type0.tsv) and [scope-combination manifest](../verification/reaper-parity/fixtures/menu-export-midi-native-empty-type0-scopes.tsv). All six empty Type 0 scopes match REAPER text and geometry; selected-track and selected-item cases with their only MIDI item outside the time selection also fail with the native scope-specific warning. Live AAADAW captures verify the same 324/377/468/521×81 layouts, and Type 0 confirmation creates no MIDI file. The same rebuilt desktop binary still produces the valid 68-byte, two-track Type 1 output. Combined Nextest passed 934/934 across 45 binaries (AAADAW 742, Core 192); strict all-target Clippy with `-D warnings`, rustfmt, CRLF-aware diff checks, and the rebuilt desktop binary all passed. The live Type 0 and Type 1 screenshots, binary ID, hashes, and output checks are archived in the scope-intersection manifest. `FILE-MIDI-EXPORT-001` remains `in_progress`; all six empty Type 0 scope modes and selected-track/item outside-time failures are now verified. Type 0 non-empty exports combined with metadata/options, additional scope intersections, keyboard/focus behavior, and full visual alignment remain open.

### 2026-10-17 Type 0 selected-track/time-selection export

REAPER 7.82 / Default 7 exported the saved scope-intersections project using Type 0, Selected Tracks, Time Selection only, PPQN 960, and all optional metadata disabled. The 36-byte format-0 file contains only the overlapping C#4 note on the selected track, rebased to tick 480. Its native MIDI, settings dialog, written-result screenshot, REAPER build/hash, and file hashes are recorded in the [scope-intersection manifest](../verification/reaper-parity/fixtures/menu-export-midi-native-scope-intersections.tsv). Added `reaper_782_type_zero_midi_export_selected_track_and_time_selection_match_native`; the focused Core Nextest passed 1/1; combined Nextest passed 935/935 across 45 binaries (AAADAW 742, Core 193), strict all-target Clippy passed, and rustfmt, CRLF-aware diff checks, and fixture path/hash validation passed. The production desktop build passed and retains Build ID `8a0f205fa560aae417ba98e06659668c91bb974c` and SHA-256 `a04ab02abecae07e1217f1e834d9bf08e961f8f9e9f813c16662605f1cf8c1d9`. No encoder change was needed because the generated file already matches the native bytes exactly.

### 2026-10-17 Type 0 time-selection export across item scopes

Native REAPER 7.82 exports were captured for Type 0 with `All`, `Selected tracks only`, and `Selected items only`, each combined with `Time selection only` at PPQN 960 and metadata unchecked. All three scopes contain only the overlapping C#4 note rebased to tick 480, producing the same 36-byte format-0 stream. The selected-item case has a saved project with only the in-range item selected; the all-items and selected-track settings and result captures are in the [scope-intersection manifest](../verification/reaper-parity/fixtures/menu-export-midi-native-scope-intersections.tsv). Three full-file Core tests passed focused Nextest. Combined Nextest passed 937/937 across 45 binaries (AAADAW 742, Core 195); strict all-target Clippy, rustfmt, CRLF-aware diff, fixture path/hash checks, and the production desktop build passed. The binary retains Build ID `8a0f205fa560aae417ba98e06659668c91bb974c` and SHA-256 `a04ab02abecae07e1217f1e834d9bf08e961f8f9e9f813c16662605f1cf8c1d9`. The encoder matches all three native outputs byte-for-byte.

### 2026-10-17 Type 0 time-selection tempo/meter metadata

REAPER 7.82 Type 0 + Selected Items + Time Selection with `Embed project tempo/time signature changes` checked exports a 51-byte merged track. It retains 4/4 and 120 BPM at output tick 0, then writes the in-range C#4 at tick 480. The native output and checked-state screenshots are recorded in the [scope-intersection manifest](../verification/reaper-parity/fixtures/menu-export-midi-native-scope-intersections.tsv). A full-file regression initially reproduced the difference: AAADAW omitted the two start events because it clipped base metadata against the time-selection start. Type 0 now keeps the base tempo/meter when this option is enabled. The new regression and all 19 Type 0 Core tests passed focused Nextest. Combined Nextest passed 938/938 across 45 binaries (AAADAW 742, Core 196); strict all-target Clippy, rustfmt, CRLF-aware diff, and fixture path/hash checks passed. The rebuilt desktop binary has Build ID `006eb6cf17afb1db9327e513b617350da4779054` and SHA-256 `9e35d2b3ec026dd443104ef66bf5572fd9bd034ba9e94c77081bd9bfefcfe44e`.

### 2026-10-17 Type 0 time-selection SMPTE origin

With Type 0 + Selected Items + Time Selection and `Embed SMPTE offset` checked, REAPER writes `00:00:01:00` at output tick 0 for a range beginning one second into a project whose configured start offset is zero. The C#4 note follows at tick 480. AAADAW previously encoded the project offset alone; it now adds the elapsed time at the range start using the project tempo map before formatting the SMPTE field. The 45-byte native file, settings/result captures, and regression are in the [scope-intersection manifest](../verification/reaper-parity/fixtures/menu-export-midi-native-scope-intersections.tsv). The new comparison passed, as did the 20 Type 0 tests and 9 SMPTE-focused tests. Combined Nextest passed 939/939 across 45 binaries (AAADAW 742, Core 197); strict all-target Clippy, rustfmt, CRLF-aware diff, fixture path/hash validation, and production build passed. The binary is Build ID `0d3ce24b6f538cce59cab2d19b6b43886a995546`, SHA-256 `692ad75bcd07792be3c04d10f4680a12a58e9786081cc5574c36f980972583e2`.

### 2026-10-17 Type 1 time-selection SMPTE origin

The Type 1 native probe confirms the same rule: with project offset zero and a one-second time-selection start, the conductor track embeds `00:00:01:00` at tick 0, then the project's 4/4 and 120 BPM start events; the selected MIDI note begins at tick 480. The 118-byte file, checked-state settings and result captures, and full-file regression are included in the [scope-intersection manifest](../verification/reaper-parity/fixtures/menu-export-midi-native-scope-intersections.tsv). The focused Core test passed. Combined Nextest passed 940/940 across 45 binaries (AAADAW 742, Core 198); strict all-target Clippy, rustfmt, CRLF-aware diff, and fixture path/hash checks passed. The rebuilt production binary remains Build ID `0d3ce24b6f538cce59cab2d19b6b43886a995546`, SHA-256 `692ad75bcd07792be3c04d10f4680a12a58e9786081cc5574c36f980972583e2`.

### 2026-10-17 MIDI export marker and controller time-selection boundaries

Linux REAPER 7.82 / Default 7 confirms format-specific behavior in the MIDI time-selection exporter. Type 0 includes individually hidden markers when `Export hidden markers` is checked, includes marker events at both selection boundaries, and retains pitch-bend/CC state from an overlapping item before the range at output tick 0. Type 1 also includes individually hidden boundary/interior markers, additionally rebases markers before the range to tick 0, and preserves conductor event ordering against the starting meter/tempo events. The native projects, 64-byte/43-byte Type 0 outputs, two 145-byte Type 1 outputs, eight REAPER dialog/result captures, and full SHA-256 manifest records are in `docs/verification/reaper-parity/fixtures/menu-export-midi-native-type0-marker-time.tsv` and `menu-export-midi-native-type1-marker-time.tsv`. Five complete-file regressions now cover the Type 0 endpoint/interior/unchecked cases and Type 1 endpoint/interior cases.

The encoder now keeps controller and pitch-bend state before an active time selection at tick 0; keeps MIDI event ranges half-open while treating marker endpoints as inclusive; applies the different Type 0/Type 1 pre-range marker rules; and orders the collapsed events to match REAPER. The MIDI export test target passed 71/71. Full workspace Nextest passed 1380/1380 across 73 binaries under Xvfb :1774, including the X11-dependent CLAP helper test. Strict workspace all-target Clippy (`-D warnings`), `cargo fmt --all -- --check`, CRLF-aware `git diff --check`, and fixture path/hash validation passed. The rebuilt production binary launched under Xvfb :90 (Build ID `5940df2ba6d40033e8e91697ca09e686966b0808`, SHA-256 `d388d965addc3ea97de685ba8d27e4e98b662351257677ecf6937d998c3555df`); its 1280×800 startup capture is `docs/verification/reaper-parity/fixtures/menu-export-midi-aaadaw-marker-time-build-smoke.png`.

### 2026-10-17 SMPTE offset with a time-selection start

The native Linux REAPER 7.82 project uses a 10.5-second configured start offset, 30 fps non-drop, and a 0.5-1.5 second time selection. Both Type 0 and Type 1 exports encode `00:00:11:00` at tick 0, confirming that elapsed range-start time is added to the configured project offset. Type 0 retains its merged SMPTE/note/controller output; Type 1 writes the same SMPTE event in the conductor track followed by 4/4 and 120 BPM. The saved [project](../verification/reaper-parity/fixtures/menu-export-midi-native-smpte-selection-offset.rpp), both complete MIDI files, checked-state dialog/result captures, and hashes are in `docs/verification/reaper-parity/fixtures/menu-export-midi-native-smpte-selection-offset.tsv`. The two new Type 0/Type 1 tests pass byte-for-byte.

The complete MIDI export test target passed 73/73; full workspace Nextest passed 1382/1382 across 73 binaries under Xvfb :1775; strict all-target workspace Clippy (`-D warnings`), `cargo fmt --all -- --check`, `git diff --check`, and fixture path/hash checks passed. The earlier Type 0/Type 1 marker and controller time-selection intersection commit is `5c5e5d6` and is pushed to `feat/reaper-parity`; these additional SMPTE regression fixtures are ready for the same branch.

### 2026-10-17 Tempo-map and time-selection intersections

Native REAPER 7.82 / Default 7 exports with a 0.5–2.5 second selection and a 90 BPM / 3⁄4 step at 2 seconds show different Type 0 and Type 1 behavior. Type 1 retains the map event at output tick 2880 after the only MIDI item's end. Type 0 emits starting 4/4 and 120 BPM before the carried pitch-bend/CC state, but stops tempo/meter events after the final MIDI event; this remains true when an item spans the change. Adding a separate D4 item at 2.125 seconds causes Type 0 to include the map change at tick 2880 before D4 at tick 3060. The time-selection end is project tick 4560 because its last half-second uses 90 BPM.

The encoder now matches the Type 0 metadata ordering and limits merged tempo/meter events to the last exported MIDI event. Three complete-file regressions compare Type 1's full selected tempo map and both Type 0 cutoff/inclusion cases. Native projects, output files, checked-state dialogs, result screenshots, and SHA-256 records are indexed in `docs/verification/reaper-parity/fixtures/menu-export-midi-native-tempo-selection.tsv`. The MIDI export test target passed 76/76; full workspace Nextest passed 1385/1385 across 73 binaries under Xvfb :1776; strict workspace all-target Clippy (`-D warnings`), `cargo fmt --all -- --check`, CRLF-aware `git diff --check`, and 15 artifact hash/reference checks passed. `FILE-MIDI-EXPORT-001` remains `in_progress`; nonlinear tempo maps, broader export-option intersections, keyboard/focus behavior, and complete visual/menu alignment remain open.

### 2026-10-17 Type 0 linear-ramp extent and event order

Linux REAPER 7.82 / Default 7 Type 0 exports now have byte-for-byte fixtures for a 120-to-60 BPM ramp. A last MIDI event at tick 1440 / 0.778402 s keeps tempo samples through tick 1800 / 0.982774 s. A MIDI note-off exactly at 1.000000 s / tick 1830 still leaves the final ramp sample at tick 1800. When the final MIDI event is tick 2100 / 1.156438 s, the ramp continues through tick 3360 / 1.923415 s and omits the tick-3480 / 2.000000 s sample. REAPER prefixes Type 0's ramp-start text with a comma; conductor samples precede same-tick MIDI while the map continues, and MIDI comes first when it shares the final conductor tick. Four saved projects, native files, dialog/result screenshots, event measurements and hashes are indexed by the Type 0 ramp evidence in the parity matrix.

The Type 0 encoder now orders merged conductor events against MIDI events across all ticks, preserves linear-ramp samples before the measured whole-second boundary, leaves step-map cutoff at the final MIDI event, and matches the terminal-tick ordering. Four full-file Core regressions cover the base, terminal-grid, exact-second, and later-content cases. The MIDI export target passed 80/80; workspace Nextest passed 1389/1389 across 73 binaries under Xvfb :2307; strict workspace all-target Clippy (`-D warnings`), rustfmt, CRLF-aware diff check, parity-matrix shape/reference checks, and native project/MIDI/screenshot hash checks passed. `FILE-MIDI-EXPORT-001` remains `in_progress` for other ramp/tempo layouts, further scope-option intersections, dialog and keyboard parity, and the rest of the REAPER interface.

### 2026-10-17 Media item mute indicator

Linux REAPER 7.82 / Default 7 shows a red `M` badge in the Arrange item header when a media item is muted. A repeatable ReaScript probe creates a selected C4 MIDI item with mute enabled; the [native capture and item crop](../verification/reaper-parity/item-mute.md) record the visible result. AAADAW's timeline cache now carries mute state for both Audio and MIDI items, and the Arrange label layer draws the same red badge when enough item width is visible. The matching AAADAW capture, project generator, binary identity and screenshot hashes are recorded alongside the native reference. The focused cache regression passed 1/1; the storage example compiles; full workspace Nextest passed 1390/1390 across 73 binaries under Xvfb :2307. The desktop was rebuilt with nightly Cranelift. REAPER playback-time mute behavior still needs a real audio-device probe because this container has no working JACK client; `ITEM-MUTE-001` remains `in_progress` for runtime/menu/property-window and full visual parity.

### 2026-10-17 Receive-side routing removal and focused Undo

Linux REAPER 7.82 / Default 7 exposes a separate Delete control for each Receive in the Receiver routing window. The native probe creates two sends to one Receiver; deleting Audio 1 leaves Audio 2 in place. The native window and before/after captures are recorded in [the native fixture](../verification/reaper-parity/fixtures/audio-receives-remove-native.tsv).

AAADAW Receive rows now provide a matching Remove button wired to the existing `DeleteAudioSend` project action. A routing-window regression verifies that removing one connection preserves the other and that one Undo/Redo restores/reapplies exactly that change. The routing child window now forwards unconsumed global shortcuts; `Ctrl+Z` with the live Receiver window focused restored the deleted row. Initial, post-delete and post-Undo desktop captures, plus the Cranelift binary identity, are in [the AAADAW fixture](../verification/reaper-parity/fixtures/audio-receives-remove-aaadaw.tsv). This confirms AAADAW operation and layout rendering; it does not claim pixel parity, and REAPER's Undo history behavior for this native Delete remains unverified.

Full workspace Nextest passed **1392/1392 across 73 binaries, 0 skipped** under Xvfb. Strict workspace all-target Clippy (`-D warnings`), `cargo fmt --all -- --check`, `git diff --check`, parity-matrix shape/ID checks, and fixture screenshot hashes passed. `ROUTE-SEND-001` remains `in_progress` for MIDI/multi-channel and other routing gaps; `INPUT-FOCUS-001` remains `in_progress` for the remaining editor/plugin/floating-window focus cases.

### 2026-10-17 MIDI Send All→All

Linux REAPER 7.82 / Default 7 的新建普通 Send 经 Lua API 实测 `I_MIDIFLAGS=0`，原厂 Routing 窗截图显示 MIDI `All → All`。AAADAW Send 默认转发 MIDI，Routing 窗可单独关闭；音符、CC、Pitch Bend 及 seek/chase 状态会按非静音 MIDI Send 路径送至接收轨道，并为并行路径副本维持独立 note ID。schema 58 为旧 Send 迁移出原厂默认开关。引擎只在非实时的 MIDI 计划编译阶段遍历路径和分配；最终 Cranelift 二进制的 Xvfb 实窗已验证开关操作，完整 workspace Nextest **1394/1394、0 skipped**、all-target Clippy `-D warnings` 通过。当前 MIDI event 模型没有通道身份，因此只覆盖 All→All；通道重映射、MIDI Bus、硬件 I/O 与 Routing Matrix 仍未完成，P3 不关闭。Folder Parent 主输出传播的后续补齐及验证状态见下一节；原厂与 AAADAW 截图、构建身份和参数探针见 [MIDI Send 验证](../verification/reaper-parity/midi-sends.md)。

### 2026-10-17 MIDI Folder Parent 路由

复核现有 solo Folder 回归后发现 MIDI 计划没有把 `effective_output_track()` 的主输出作为事件目的轨，尽管音频图和此前验证文档已将 Folder Parent 作为有效路由。现在 Note、CC、Pitch Bend 与 chase 会沿启用的主输出逐层进入 Folder 父轨；任一层关闭 Main Send 后，该层之后不再收到事件。新增嵌套 Parent 与逐层开关回归，并扩展 solo Folder 回归以检查事件确实到达 Folder。REAPER 7.82 / Default 7 原厂专项离线 MIDI 接收计数也已完成：两层 Parent 开启时顶层 Folder 收到 1/1 个 Note On/Off，关闭叶轨或中间 Folder Main Send 后均为 0/0，工程与探针见 [Folder Parent MIDI 验证](../verification/reaper-parity/folder-parent-midi.md)。AAADAW 路由遍历只在 MIDI 计划编译时运行。workspace Nextest **1395/1395、0 skipped**、严格 all-target Clippy 与 nightly Cranelift 桌面构建通过。

### 2026-10-17 实时 MIDI 通道身份

实时 MIDI 解码和 `ScheduledMidiEvent` 现保留 0–15 通道；CLAP Note PCKN、CC/Pitch Bend 原始消息、活动音符清理和固定大小 helper IPC 都使用相同通道。隔离 helper 协议升为版本 4，但 MIDI 记录仍为 16 字节；Android MIDI 状态字节保留通道并拒绝非法值。既有工程计划事件仍为通道 0，持久化 MIDI channel、通道编辑及 Send 映射继续未实现，P3 未关闭。回归验证包括跨通道同音高音符配对、CC/Pitch Bend 与 IPC；Nextest **1396/1396、0 skipped**，严格 workspace Clippy、格式、差异检查通过。细节见[实时 MIDI 通道验证](../verification/reaper-parity/live-midi-channels.md)。

### 2026-10-17 工程 MIDI 通道身份与 schema 59

工程 Note、CC、Pitch Bend 数据新增 0..15 通道身份，Core 快照校验、复制/分割、控制器与 Pitch Bend chase、MIDI 计划、Send All→All 路径及标准 MIDI 导出均保留通道。SQLite schema 59 为旧工程迁移出通道 0，并扩展 CC/Pitch Bend 的唯一键，逐表兼容已有列和迁移夹具中的缺失旧表。MCP 的 Note/表达式查询返回通道，Note 编辑支持显式切换且省略时保留原通道；Note 插入和 CC/Pitch Bend upsert 支持可选通道，缺省兼容通道 0。无通道选择器的 MIDI 编辑器仍以通道 0 新建事件，拖动已有 CC/Pitch Bend 保留事件原通道，重复位置校验按通道区分。

专项回归覆盖同 tick 多通道事件、标准 MIDI 通道状态字节、计划/seek-chase、快照重开、schema 58→59 及更旧/部分旧表迁移、MCP 持久化编辑和编辑器通道键控。workspace Nextest **1400/1400、0 skipped，74 binaries**（限制 4 个并行 worker）通过；workspace all-target Clippy `-D warnings`、格式检查、`git diff --check` 以及 nightly Cranelift 桌面构建通过。Cranelift 编译保留 iced/WGPU auto-trait 的既有未来兼容警告。该数据模型切片没有新增视觉控件，因此未做 AAADAW/REAPER 截图比对或原厂 UI probe。Send 通道筛选/重映射、MIDI Bus、编辑器通道选择、其他 MIDI 消息和 P3 完整路由/界面验收仍未完成；`ROUTE-SEND-001` 继续为 `in_progress`。

### 2026-10-17 MIDI Editor 通道选择与筛选

Linux REAPER 7.82 / Default 7 实窗确认 MIDI Editor 菜单含 All channels、Channel 1–16 与 Multichannel；单通道选择后显示 Filter，启用时 note/CC/Pitch Bend 其他通道隐藏，Multichannel 打开事件筛选窗并独立选择可见通道和绘制通道。AAADAW 已接入对应 18 项选择器、Filter 开关、多通道集合与绘制目标；钢琴卷帘及表达式 lane 的绘制、命中/选择和事件新增遵循这些设置，切换筛选会清除隐藏音符选择。原厂和 AAADAW 实窗截图、事件/settings TSV、probe 脚本及 13 项 SHA-256 清单见 [通道编辑器验证](../verification/reaper-parity/midi-channel-editor.md)。

回归覆盖全部/单/多通道匹配、18 个菜单选项、隐藏音符命中、CC/Pitch Bend 筛选和新事件通道。workspace Nextest **1404 tests，74 binaries**（4 workers）通过；workspace all-target Clippy `-D warnings`、`cargo fmt --all -- --check`、`git diff --check` 及 13 项证据 hash 校验通过。夜间 Cranelift Xvfb 实窗截图确认 All channels 隐藏 Filter、Channel 2 筛选后仅显示匹配事件，以及 Multichannel 通道集合/绘制目标交互。`MIDI-CHANNEL-VIEW-001`、UI-018/UI-019 与 P7 仍为 `in_progress`；原厂 invert/solo 和更多事件条件、颜色、多窗口几何，以及 MIDI Editor 其余高阶功能尚未实现。

### 2026-10-17 MIDI Editor 选中音符网格量化

Linux REAPER 7.82 / Default 7 原厂探针调用 MIDI Editor action 40469，确认新配置默认网格为 1/32（PPQ 960 下每格 120 tick）；三个选中音符的起点分别由 100/350/900 移到 120/360/960，未选音符 550 不动，音符长度、velocity 与通道保持。命令输出、原厂量化前后工程、REAPER 截图和 ReaScript 见 [量化验证](../verification/reaper-parity/midi-note-quantize.md)。

AAADAW Piano Roll 新增 Quantize 按钮，以当前编辑网格和 100% 力度只量化选中音符起点，并复用单个 `MidiNotesQuantized` 项目事件实现 Undo/Redo。新建工程和无已保存视图状态的项目默认 MIDI 网格改为 1/32；显式保存的网格不变。所有 15 种普通、附点与三连音网格到 PPQ tick 的转换均有回归。Cranelift Xvfb 实窗截图显示 Quantize 控件与 1/32 网格；二进制 Build ID `da45f1ef2f96b5fd6502c230ac33ead45e35417f`。workspace Nextest **1407 tests，74 binaries**（4 workers）通过，严格 workspace all-target Clippy、格式/差异检查、104 行唯一 ID/15 列 parity matrix 校验及本切片 8 项 SHA-256 检查通过。完整 Quantize 对话框、力度/事件量化、Swing 交集、Humanize 以及其他 MIDI Editor 模式未完成；音符结尾量化由下一节单独覆盖。`MIDI-NOTE-QUANTIZE-001`、UI-018/UI-019 与 P7 继续为 `in_progress`。

### 2026-10-17 MIDI Editor 选中音符起点与结尾量化

Linux REAPER 7.82 / Default 7 原厂 probe 调用 MIDI Editor action 40729，确认 1/32 网格下选中音符的起点与结束分别吸附：100–220→120–240、350–430→360–480。短音符 900–970 若两端落在同一网格线，原厂将结束扩至下一格，得到 960–1080；未选音符 550–670 保持不变，通道和 velocity 不变。完整 native event TSV、量化前后工程、ReaScript、AAADAW 工程/源码及两端截图哈希见[起点与结尾量化验证](../verification/reaper-parity/midi-note-quantize-ends.md)。

AAADAW MIDI Editor 工具栏新增 `Q pos+end`，仅处理选中音符；起点、时值一起保存为单个 `MidiNotesQuantized` 事件，单步 Undo/Redo 同时恢复/重做两者。Core 回归比较原厂三组端点并覆盖被选择集合过滤、最小一格音长、音符通道/velocity 和历史；App 回归验证真实消息路径。夜间 Cranelift 桌面实窗截图确认按钮显示于 MIDI Editor 1/32 网格旁，Build ID `b3d8d140ee86c079c427beb934e7d2cd6fa4baf8`。workspace Nextest **1409 tests，74 binaries** 通过，严格 workspace all-target Clippy、格式/差异检查、105 行唯一 ID/15 列 parity matrix 校验及本切片 8 项 SHA-256 检查通过。完整 Quantize dialog/力度设置、Swing 网格交集、事件量化与 Humanize 仍未覆盖；`MIDI-NOTE-QUANTIZE-END-001`、UI-018/UI-019 与 P7 继续为 `in_progress`。

### 2026-10-17 MIDI Editor Quantize Events 对话框（部分覆盖）

Linux REAPER 7.82 / Default 7 action 40009 的实窗截图确认默认设置，并记录全部 scope 与 operation 菜单。实测 1/32、100% 下五种原厂音符模式的 PPQ 输入/输出；尤其用 100–280 音符区分 `Position and note end`（120–240）与 `Position and note length`（120–360），另验证 `Note end only`、`Note length only` 会保持起点。读数、可复现 ReaScript 和操作数值见[对话框验证资料](../verification/reaper-parity/midi-note-quantize-dialog.md)。

AAADAW 对话框现提供 Selected/All notes、全部五种原厂音符操作、0–100% strength 与 Commit/OK/Cancel/Escape；原有快速 `Q pos`、`Q pos+end` 仍保留。Core 通过一个量化操作动作记录位置和长度变化，所有五种模式的选中范围、tick 结果及 Undo/Redo 均有回归。

后续原厂 action 40009 probe 确认 All events 的 Position 操作会移动已选/未选音符、CC 与 Pitch Bend 的位置并保留值、通道、选中状态和音符长度。AAADAW 新增跨现有三种事件数据的单次可撤销编辑。第二次原厂 probe 进一步确认：CC1 在 475/485 tick 量化到 480 后保留 value 65；Pitch Bend 在 835/850 tick 量化到 840 后保留 value 8192。也就是说，同通道同 CC 参数的冲突、以及同通道 Pitch Bend 冲突都会保留原始 tick 较晚者；Core/App 按同一规则合并，Undo 会恢复被合并事件。第三次原厂 probe 确认 All events 下勾选 Bypass 后按 OK，已选/未选音符、CC 与 Pitch Bend 的 tick 和数值全部不变。AAADAW 新增默认关闭的 Bypass 复选框；Commit 不执行编辑且保持对话框，OK 不执行编辑并关闭。Nightly Cranelift 二进制 Build ID `95aa3786a7a68c31f780d6c8723d2d0d0742fc6e`、SHA-256 `415de83dd850be9bbbffe1e4fa23f223fdcefc6147a1c1b98a2dafba49e50231`；原厂读数、两端实窗截图和哈希见[对话框验证资料](../verification/reaper-parity/midi-note-quantize-dialog.md)。Workspace Nextest **1417 tests，74 binaries** 全部通过，包含 Core 五种音符模式/All events tick 与 Undo/Redo、App Bypass/Commit/OK 和对话框路径；严格 workspace all-target Clippy (`-D warnings`)、fmt、diff 检查、106 ID/15 列 matrix 和 13 个量化截图哈希均通过。
第四次原厂 probe 对比 Fix overlaps 勾选/未勾选：Position and note length、1/32、100% 下，勾选会把首个量化音符截短到下一个同通道同音高音符的起点；关闭后保留重叠。AAADAW 对话框现按原厂默认勾选并接入 Core，撤销/重做仍为一个事件。实测还确认正强度 Note length 量化将舍入为零的短音符提升到至少一格；0% 仍保持原始时值。原厂勾选/未勾选读数与截图、Cranelift 桌面新截图和哈希均记录在[对话框验证资料](../verification/reaper-parity/midi-note-quantize-dialog.md)。
Selected notes 的额外探针确认，勾选 Fix overlaps 时即使仅选中后一个音符，也会截短前一个未选中同音符；该前置音符变化纳入一次撤销。关闭选项时，REAPER 在这组重叠输入中保留所选音符原始 end，AAADAW 当前关闭路径仍未匹配这一交互，故明确列入剩余差异。

本次最终 workspace Nextest **1420 tests / 74 binaries，0 skipped**，Core MIDI quantize 7/7、App quantize 7/7；严格 workspace all-target Clippy (`-D warnings`)、fmt、diff 检查及 17 个量化截图哈希通过。对话框 UI、菜单和应用结果已在新 Cranelift 桌面二进制实窗复核。

`MIDI-QUANTIZE-DIALOG-001`、UI-019 与 P7 仍为 `in_progress`：Selected events、Program Change/channel pressure/SysEx 等未建模 MIDI 事件、selected-scope 关闭 Fix overlaps 时的复杂多选/混合 note-length 链与已记忆对话框设置尚未覆盖；Swing 交集已由后续切片补齐。


### 2026-10-17 MIDI Quantize Events Swing 网格支持

REAPER 7.82 / Default 7 以 1/32、PPQ 960、50% Swing 实测所有五种音符操作：奇数网格点向后偏移 30 tick；Position 和两种端点操作吸附 Swing 点，Note length only 与 Position and note length 的长度仍使用直线 120-tick 网格；75 tick 等距时选择较晚的 150。All events scope 下，未选音符、CC、Pitch Bend 同样使用 Swing 网格，CC/Pitch Bend 同 tick 冲突保留原始位置较晚事件。数据与真实对话框截图见 [Swing 原厂 fixture](../verification/reaper-parity/fixtures/midi-quantize-swing-native.tsv) 和 [探针](../../scripts/reaper_parity/probe-midi-quantize-dialog-swing.lua)。

Core 将 Swing 百分比纳入量化设置，覆盖五种操作、正/负 Swing、strength、All events 与 Undo；App 将当前活动 Swing 网格传入 note/event action。新增回归直接比较原厂 tick。当前源码以 nightly Rust 1.101.0 + Cranelift 构建，Build ID `da8e46fd6e54e648d6039ce219883c4e9934c1c0`，SHA-256 `295e44501858a44c488deb9482c7b97dfba7024bfb3a3e2331a094d69e67bace`；在 Xvfb :260 实际打开 MIDI 项目、启用 Swing 50%、选择 All notes → Position 并应用。Swing 设置、Quantize 对话框和应用后状态见 [AAADAW 实窗 fixture](../verification/reaper-parity/fixtures/midi-quantize-swing-aaadaw.tsv)。workspace Nextest **1423 tests / 74 binaries，0 skipped**，严格 workspace all-target Clippy (`-D warnings`)、fmt、diff、106 ID/15 列 parity matrix 与 22 张量化截图哈希通过。

`MIDI-QUANTIZE-DIALOG-001`、UI-019 与 P7 仍为 `in_progress`：Selected events、Program Change/channel pressure/SysEx 等未建模类型、selected-scope 关闭 Fix overlaps 的复杂 note-length 链，以及已记忆对话框设置仍待覆盖。


### 2026-10-17 MIDI Quantize Selected events 范围

Linux REAPER 7.82 / Default 7 实测 Selected events：只量化已选音符、CC 与 Pitch Bend；选中 CC/Pitch Bend 与未选事件落到同一 tick 时，原厂保留选中事件，即便未选事件原始 tick 更晚，且量化后保留选择标记。原厂逐事件前后读数、对话框截图和 ReaScript probe 已归档。

AAADAW 新增 Selected events 范围及 MIDI CC/Pitch Bend 点击选择、Ctrl/Command-click 增减选择和琥珀色选中态；scope 顺序对齐原厂。实机发现对话框外展开的下拉项会被模态遮罩误收为取消，已拆分遮罩与对话框叠层。最新 Cranelift Build ID `6dde40f577b548aa1b9aa2bfc6d37588b0e19f94`、SHA-256 `9377421daf09fdbc47cc18134d7f0328106c14a2a580873f2e4e909de97b113e` 下，Xvfb 实窗完成离网格 CC1 点量化：所选点吸附并保留选择，未选 CC 点不移动；对话框、编辑器和应用结果截图哈希见 [fixture manifest](../verification/reaper-parity/fixtures/midi-quantize-dialog-ui.tsv)。

Workspace Nextest **1426 tests / 74 binaries，0 skipped**（`cargo xtest --no-default-features`，Xorg dummy display :286）；Core MIDI quantize 9/9、App Quantize 11/11、严格 Core/App all-target Clippy (`-D warnings`)、fmt、diff 检查、106 行/15 列唯一 ID parity matrix 和本切片截图 SHA-256 检查全部通过。Selected events 已实现，但 MIDI Program Change/channel pressure/SysEx 等未建模事件、Fix overlaps 关闭时多个选中音符/多个 note-off 穿越及其他混合选择的同音高重叠链，以及已记忆对话框设置仍未覆盖；`MIDI-QUANTIZE-DIALOG-001`、UI-019 与 P7 保持 `in_progress`。

### 2026-10-17 Selected-note overlap chain Quantize readback

REAPER 7.82 / Default 7 在 Selected notes、Position and note length、100%、Fix overlaps 关闭时，对五个同通道同音高音符完成原厂探测。只选中第二个音符后，raw selected note-off 从 300 移到 360；`MIDI_GetNote` 仍报告选中音符结束于 300，并把下一未选音符从 `330–480` 读回为 `330–360`。原始事件流、完整 note-table 前后读数、可复现 ReaScript 和对话框/应用截图见 [overlap-chain fixture](../verification/reaper-parity/fixtures/midi-quantize-selected-overlap-chain-native.tsv)。该样本在量化前已有一个 note-table 结束 tick 与 raw note-off 不一致，fixture 明确保留这一条件。

Core 在选中音符与更早的未选同音音符重叠、且 Position 或 Position and note length 的新结束越过其后的未选同音音符时，保持所选音符原结束并反映 REAPER 读回的邻接音符结束；App 的 Quantize dialog 路径也覆盖同一结果和 Undo。Core MIDI quantize suite **10/10**、AAADAW App suite **743/743**、Core/App all-target Clippy `-D warnings`、fmt 通过。新源码用 nightly Cranelift 重建，Build ID `e1b422b9ac597a79b6102af33a548141a9ea8df8`，SHA-256 `61982fdc37acaa961c6c827d7740824fc5d1ef98731d06d13f636daa0d0259ac`；Xorg dummy :288 实窗打开五音项目和 Quantize dialog，current-build 截图及项目 hash 见 [fixture manifest](../verification/reaper-parity/fixtures/midi-quantize-dialog-ui.tsv)。此次没有把整组量化项标记完成：多个选中音符、多次结束事件穿越、其他操作及混合选择链仍需原厂探测；`MIDI-QUANTIZE-DIALOG-001`、UI-019 与 P7 继续为 `in_progress`。

### 2026-10-17 MIDI Editor Humanize Notes（部分实现）

Linux REAPER 7.82 / Default 7 的 action 40457 实窗确认默认作用范围为 Selected notes，Timing、Velocity、Timing bias 初始均为 0%；Timing/Velocity 范围 0–100%，bias 为 −100–100%，并提供 New random seed。两次 100% Timing/Velocity、0% bias 的读回均保持音符长度及未选音符不变；显式重新取种子的样本让起点分别偏移 0、+31、+7 tick，velocity 分别变化 −29、−17、+3。独立 100% bias 样本出现 +367、−86、+266 tick 位移，证明 bias 可超出半网格范围。完整 note/event 读数、前后 RPP、原厂对话框截图及 ReaScript probe 见 [Humanize 验证资料](../verification/reaper-parity/midi-humanize.md)。这些随机样本只约束不变量和可见范围，尚不能推定原厂精确分布。

AAADAW MIDI Editor 已加入 All notes/Selected notes、Timing、Velocity、Timing bias、New random seed、OK/Cancel；Core 用一个历史事件保存一次计算出的 note start/velocity 变化，支持稳定 Undo/Redo、范围校验、音长保持与未选隔离。AAADAW 当前使用本地 seeded generator；随机曲线仍是待校准模型。当前 nightly Cranelift 桌面 Build ID `9c3ab6f1bcbb98ccaa375fda51ad4aa5fbc5880f`，SHA-256 `64b9aadc2e7ea42bf9f133c19072f9c649ab85023dc5d9f7bb5fe70ef70d0f4d`；在 Xorg dummy :289 实窗打开 MIDI Editor，当前二进制默认对话框、100% Timing/Velocity 配置和执行后状态的截图 hash 为 `2661100f5d06d9060de15b417e664eeacf25faa621ef5e5b5ac3e3d2c10e7cb9`、`7c00115b9d54d76f76c9f846a901d2649e9c6670e42f999e12ec4402dfa34bca`、`665fef4335a75c95d61567c12d60461fac9162f67e9022735700ed9111f76ae1`。

全量 Core 测试 **226 tests / 42 suites**、AAADAW App **744/744**、Core/App all-target Clippy (`-D warnings`) 通过；本次重新格式化源码。`MIDI-HUMANIZE-001` 保持 `in_progress`：后续要探测更多原厂 seed、低/高 velocity 边界及 bias 分布，再调整本地曲线；New random seed 的原厂语义和已记忆 dialog settings 也未验证。P7、UI-019 继续开放。

### 2026-10-17 MIDI Humanize 对话框设置记忆

Linux REAPER 7.82 / Default 7 实测：确认 All notes、Timing 37%、Velocity 61%、Timing bias −42% 后，`reaper.ini` 保存 `humanall=1`、`humantiming=37`、`humanvel=61`、`humanbias=-42`；使用同一隔离 profile 重启，原厂窗口恢复这组值。随后临时改为 Selected notes / 12% / 23% / 0% 并点 Cancel，配置仍保留最后确认值。读数、配置项和原厂截图见 [Humanize persistence fixture](../verification/reaper-parity/fixtures/midi-humanize-settings-native.tsv)。

AAADAW 现把这些值保存到版本化全局 `midi-humanize-preferences.json`，通过临时文件原子写入；只有 OK 会更新并安排保存，Cancel/Escape 保留上次确认值。启动时恢复配置，退出时刷新待写设置；无效版本和超范围值不会覆盖原配置。App 回归覆盖默认值、Cancel 不提交、OK 更新、重新打开恢复；偏好模块回归覆盖读写及无效文件保护。夜间 Cranelift Build ID `d8ab03b30c7e342bbe69412b51f092b31cf42ccd`、SHA-256 `71f667150d23eec0363788308b72aa38c74bd80cb30e63c206bd06c1274f323c`；Xorg dummy :289 中用独立 XDG profile 启动当前桌面二进制，实际窗口恢复 All/37/61/−42，截图 SHA-256 `07ef32b4ca4eed80afd55272dfc4f9aa0d1be40118b3e802beeb9d004668e585`。

AAADAW App 全套 **746/746**、Humanize preferences 定向 2/2、Core/App strict Clippy (`-D warnings`)、fmt 与 diff 检查通过。`MIDI-HUMANIZE-001` 仍为 `in_progress`：timing/velocity/bias 的精确原厂随机曲线、低/高 velocity 边界和 New random seed 序列尚未匹配；P7 与 UI-019 继续开放。

### 2026-10-17 MIDI Humanize timing distribution 校准

在 REAPER 7.82 / Default 7、960 PPQ、120 BPM、MIDI 与项目 1/32 网格下，重新采集 64 和 512 个音符两份 100% Timing / 0% Velocity / 0% bias 样本，并保存设置截图、应用后截图与逐音符前后读数。合计 576 个选中音符的位移均值 −1.35 tick、标准差 122.31 tick、范围 −373 到 +400 tick，分布接近对称，支持零 bias 时的 PPQ/8 尺度；音长、pitch、channel、velocity、selection 和未选音符保持不变。

原厂 bias 端点仍未形成可复现的单一关系：64 音符 +100% 的均值为 +122.8 tick，−99% 为 −131.4 tick；512 音符重复样本中，+100% 两次均值分别为 −126.8 与 +121.3 tick，−100% 两次为 −5.9 与 +111.3 tick。各次已保存设置截图与前后读数；这些结果不足以支持一个确定的 bias 模型，因此 AAADAW 当前 ±PPQ/8 线性偏移仍是临时近似，Humanize 继续为 `in_progress`。

Core 当前 timing 模型使用标准差 `project PPQ / 8` 的正态抽样、按 Timing 百分比缩放，并在四个标准差处截断；Bias 模型单独标为临时近似。新的 REAPER 探针可设置 note count、velocity、tempo、MIDI/project grid，并导出事件前后读数。

当前代码由 nightly Rust 1.101.0 Cranelift 构建，Build ID `9efbe45b29a0ff959422bd60affe6e86c084bf2f`，SHA-256 `1aa3f584b7916b1f52181c8f0cf39debe878b5ec2c04b264ce6edc9b48075fe8`。Xorg :90 验证新二进制启动；Xorg :290 实际创建 MIDI item 与 note、打开 Humanize、应用 100% Timing / 0% Velocity / 0% bias。启动、默认 dialog、配置与应用后截图及哈希见 [当前 Humanize Cranelift fixtures](../verification/reaper-parity/midi-humanize.md)。Cranelift 报告 `TimelinePipeline: MaybeSync` 的未来兼容递归 trait 警告，构建成功。

Core 全套 **228 tests / 41 binaries** 通过；最后收紧后的 Humanize Core 定向套件 **5/5**；AAADAW App **767/767**；Core/App all-target Clippy (`-D warnings`) 与 `cargo fmt --all -- --check` 通过。`MIDI-HUMANIZE-001`、UI-019 与 P7 仍开放：Bias 原厂语义、velocity 分布/边界、精确 RNG 和 New random seed 行为没有关闭。

### 2026-10-17 MIDI Humanize 偏置预置复测与当前桌面验证

复查先前 bias 样本时发现测量方法问题：在当前 REAPER Humanize 对话框内修改 slider 后，`reaper.ini` 会保存新值供下一次调用，但当前 action 仍可能使用上次确认的值。因此旧的会话内 slider-edit endpoint 截图不能证明该次 action 应用了截图显示的 bias；raw TSV 仍保留，但从 bias 拟合中排除。

随后用 REAPER 7.82 / Default 7、每次启动前独立预置 `reaper.ini` 的方式采集 7 份 512 选中音符样本（每份另带 1 个未选控制音符），打开对话框后不改值并立即应用：bias +100% 两次 pooled 均值 +117.90 tick / SD 119.68，−100% 两次 −119.12 / 120.56；+50% 一次 +63.39 / 118.45，−50% 两次 −52.72 / 122.65。0 bias 的既有 576 音符样本均值 −1.35、SD 122.31。四个正负 bias 中点/端点均值与 `bias% × PPQ/8` 的约线性偏移一致，离散尺度仍接近 PPQ/8；音长、pitch、channel、velocity、selection 和未选控制音符保持不变。各次设置截图、逐音符前后读回、配置 manifest 和 SHA-256 见[Humanize 验证资料](../verification/reaper-parity/midi-humanize.md)。

此前的 velocity 扩展样本也采用启动前预置值：100% 3 次、50% 2 次、0% 1 次，共 3,072 个选中音符读回，支持近似正态尺度 `initial velocity × 0.23 × amount%` 及 MIDI velocity 1–127 clamp；0% 样本全部不变。当前 Core 模型使用相同尺度和 `bias% × PPQ/8 × timing%` 的 timing 均值偏移。原厂精确 RNG、New random seed 序列和 slider 修改后当前 invocation 的完整语义仍未关闭，`MIDI-HUMANIZE-001` 继续为 `in_progress`。

nightly Cranelift 重建后的桌面二进制 Build ID `91ba39a553698e12b82712452d6e44e7b02959d4`、SHA-256 `0443ccf711827af62243b3beb0a5d9f3a8e474f422e007dd036c288c07b83ef8`。Xvfb :291 实际启动该二进制、创建 track/MIDI item/note、打开 Humanize Notes、设为 Timing 100% / Velocity 0% / bias 0%，并在 OK 后返回编辑器；启动、对话框、配置与应用后截图及哈希已纳入验证资料。

Core Nextest **229 tests / 41 binaries** 通过，Humanize Core 定向 **6/6**，AAADAW App **767/767**；Core/App all-target Clippy (`-D warnings`)、`cargo fmt --all -- --check`、`git diff --check` 和 parity matrix **107 项 / 15 列 / 唯一 ID** 校验通过。`MIDI-HUMANIZE-001`、UI-019 与 P7 仍开放，直到精确随机序列、seed 和 invocation 语义达到验收标准。

### 2026-10-17 MIDI Editor Multichannel 反向筛选

REAPER 7.82 / Default 7 的 `Filter Events` 窗口实测：勾选 Channel 1 和 3–16、取消 Channel 2 并启用 “Show only events that pass filter” 时，Invert 关闭显示 4 个匹配音符；启用 Invert 后通道勾选集不变，只显示互补的 2 个 Channel 2 音符。原厂成对截图、事件 tick/channel 读数及复现探针见 [MIDI channel fixture manifest](../verification/reaper-parity/fixtures/midi-channel-editor-ui.tsv)。

AAADAW 的 Multichannel 面板新增原厂同名 Show only 与 Invert 控件；Invert 对当前通道集合取反并驱动音符、CC、Pitch Bend 的显示和命中测试，改变筛选后清除变为隐藏的音符选择。App 回归验证互补集合与选中项清理；Piano Roll、CC 和 Pitch Bend lane 分别覆盖反向命中路径。nightly Cranelift 当前二进制 Build ID `42006dc9d7d0b4c8d2caa280cb0457955388dc5d`，SHA-256 `f3d657dca6cfdb19ff6e4300e3962f46df80e4a2fe7047567d78c4b52efd902c`；Xvfb :294 实窗截图显示同一通道集合下 4→2 个可见音符，截图哈希与原厂参考均列入 fixture manifest。

AAADAW App Nextest **768/768，0 skipped**、all-target Clippy (`-D warnings`)、`cargo fmt --all -- --check` 通过。`MIDI-CHANNEL-VIEW-001` 保持 `in_progress`：原厂 Solo、参数/值/位置/长度/事件类型条件、轨道列表、通道颜色及单独浮动窗口和完整几何仍未实现；UI-018、UI-019 与 P7 继续开放。

### 2026-10-18 MIDI Editor Filter Events 事件类型

Linux REAPER 7.82 / Default 7 原厂 `Filter Events` 实测提供 `<all events>`、Note、Poly Aftertouch、Control Change (CC)、Program Change (PC)、Channel Pressure、Pitch、Sysex/Meta 八类事件条件。保持全部 16 个通道勾选并启用 Show only 时，Note 显示 4 个音符，CC 显示 3 个 CC20 点，Pitch 显示 3 个 Pitch Bend；Note + Invert 隐藏全部音符并让 Pitch Bend 通过，说明 Invert 对通道和事件类型的组合条件整体取反。原厂菜单、三种类型结果、反向结果、逐事件读数及桌面验证文件已收入 [MIDI channel fixture manifest](../verification/reaper-parity/fixtures/midi-channel-editor-ui.tsv)。

AAADAW 在 All channels 模式下增加 `Filter events…` 入口，首次打开将 16 个通道全部勾选。事件类型已覆盖 `<all events>`、Note、Control Change (CC)、Pitch；默认 `<all events>`。统一谓词驱动音符计数、绘制/命中、CC 与 Pitch Bend lane 绘制/命中；Invert 取反组合匹配，修改筛选时会清除被隐藏音符的选择。可复现项目生成器及六个音符、三条 CC1、三条 Pitch Bend 的清单记录在同一 fixture manifest。Poly Aftertouch、Program Change、Channel Pressure、Sysex/Meta、Solo、参数/值/位置/长度筛选、轨道列表、通道颜色、单独窗口控件和完整几何均未实现，本项继续为 `in_progress`。

nightly Cranelift 桌面 Build ID `59cb80e57dc54292b25558be33a1b619ccb568d7`、SHA-256 `6b63619599cc55599121c4656973dfbf7518b6e2cad0d1b5c63a5ffe9bb6787f` 在 Xvfb 1280×800 实际运行。截图捕获了类型菜单、选择项、零音符计数以及 CC/Pitch lane。当前 detached editor 的 Xvfb 截图同时含有主窗口 Arrangement 预览的浅绿色音符像素，因此不把截图当作 Piano Roll 空白像素证明；组合谓词、计数和 hit-test 由回归测试验证。全工作区 Nextest **1439/1439，0 skipped**（Xvfb）、AAADAW all-target Clippy (`-D warnings`)、`cargo fmt --all -- --check`、`git diff --check` 均通过；parity matrix 校验为 **107 项 / 15 列 / 唯一 ID**，fixture manifest **36 个条目**全部存在且 SHA-256 一致。


### 2026-10-18 MIDI Editor 音符半音与八度动作

Linux REAPER 7.82 / Default 7 的 MIDI Editor action 40177/40178 对所选音符上/下移动一个半音，40179/40180 移动一个八度；41026/41027 是忽略 scale/key 的半音动作。默认无 scale/key 限制的项目内，选中音符按 ±1 / ±12 变化，未选音符及音符起止、通道、力度、选择保持不变。另测到所选集合中任一目标音高越过 0–127（pitch 127 上移、pitch 0 下移或混合集合）时，REAPER 整批不做部分移动。逐事件读数与复现探针见 [MIDI transpose fixture manifest](../verification/reaper-parity/fixtures/midi-note-transpose-ui.tsv)。

AAADAW 将六个动作及 Linux REAPER 默认 NumPad、Ctrl/Alt+Arrow 绑定加入 Action List / MIDI Editor 键盘分发。选中音符沿 Core `EditMidiNote` batch 一次编辑，单步 Undo/Redo；超出音高范围时整批不动。App 回归覆盖快捷键分派、音符元数据保持、未选项隔离、边界拒绝和撤销重做。夜间 Cranelift/普通 GUI 构建不依赖此次 Core/App 音频 callback；此切片没有新增实时音频路径。

MIDI Editor scale/key 尚未建模，因此 scale-aware 与 ignore-scale 动作目前在默认无 scale/key 状态下等价；Action 40759 `Transpose notes…` 对话框和 cent 偏移仍未覆盖，`MIDI-NOTE-TRANSPOSE-001`、UI-019、P7 保持 `in_progress`。

AAADAW App Nextest **770/770，0 skipped**（Xvfb；在最后补充物理 NumPad 事件断言前）；补充后该切片的完整回归 **1/1** 再次通过。strict all-target Clippy (`-D warnings`)、`cargo fmt --all -- --check`、`git diff --check` 通过；fixture manifest **2/2 个条目 SHA-256 匹配**，parity matrix 为 **108 项 / 15 列 / 唯一 ID**。随后重复全套链接因编译期磁盘耗尽失败；清理仓库桌面烟测脚本规定的可再生成 AAADAW 链接临时文件后，聚焦回归与 Clippy 正常通过。

### 2026-10-18 MIDI Editor Transpose MIDI 对话框

Linux REAPER 7.82 / Default 7 的 MIDI Editor action 40759 默认键位为 `T`。原厂 `Transpose MIDI` 初始显示 `Selected notes`、半音 `0`、`Snap to scale` 关闭、From/To C Major 与 `+0 octaves`。`All notes` 会把未选音符一并纳入；`+2` 在 Selected notes 下只改变选中音高，在 All notes 下也改变未选 pitch 72→74。修改半音数时原厂立即预览，Cancel 恢复原 pitch；Cancel 后再打开，All notes 范围仍保留但半音回到 0。默认与 Cancel 后窗口、事件逐项读回及探针见 [Transpose MIDI fixture manifest](../verification/reaper-parity/fixtures/midi-note-transpose-dialog-ui.tsv)。

AAADAW Action List 新增 `midi-editor.transpose-notes` / `T`。`Transpose…` 窗口当前提供 Selected/All notes 与有符号半音输入，实时映射 Piano Roll 显示；工程模型在对话框期间保持不变，Cancel/Escape 清除预览，OK 用单个 `EditMidiNotes` 历史事件提交 pitch-only 变更。范围在 App 会话内记忆，每次对话框把半音重置为 0。测试覆盖默认快捷键、作用范围、预览/Cancel 状态隔离、All notes 一次事务、Undo/Redo 及音符属性保持。

AAADAW 最新 Cranelift 桌面二进制已在 Xvfb 中启动并显示 MIDI Editor 对话框；[实窗截图](../verification/reaper-parity/fixtures/midi-note-transpose-dialog-aaadaw.png)记录 All notes / +2 状态（Build ID `0a27d42e4b67484760aeb0a35c3638a9ea1dd336`）。它只验证真实窗口与控件布局，不替代自动化音高预览、取消及提交验证。

App Nextest **772/772，0 skipped**，严格 AAADAW all-target Clippy、`cargo fmt --all -- --check`、`git diff --check` 通过；native 和 AAADAW fixture manifest **5/5 个条目 SHA-256 匹配**。原厂 `Snap to scale`、From/To scale/key 和 octave 控件、跨启动 scope 设置、40759 自身的 pitch 越界行为均未完成或验证，`MIDI-NOTE-TRANSPOSE-DIALOG-001`、UI-019、P7 继续 `in_progress`。


### 2026-10-18 MIDI Editor Event List 基础模式

Linux REAPER 7.82 / Default 7 的 MIDI Editor action 40056 在 Alt+3 打开 Event List，Alt+1 返回 Piano Roll。隔离配置下运行的 ReaScript 实际读回 4 个音符、0 个 CC/text event：PPQ ticks 分别为 0、240、480、720，长度均为 120，显示通道 2–5、C4/E4/G4/C5 及力度 91–94。原厂实窗列出 Index、Position、Length、Channel、Type、Parameter、Value、Off Value、REAPER Data，并在 MIDI item 终点显示 `(end) All Notes Off`；复现脚本、原厂读回、两张实窗截图和 SHA-256 见 [Event List fixture manifest](../verification/reaper-parity/fixtures/midi-event-list-ui.tsv)。

AAADAW 新增 Event List/Piano Roll 两种编辑模式、对应 Action List 动作与默认快捷键；列表现显示工程 BBT 的 Note、CC、Pitch Bend 行，使用现有 channel/type filters。点击和 Ctrl/Cmd-click 可单选/多选，Delete 将选中建模事件合并为一个可 Undo/Redo 的事务。当前 Cranelift 桌面 Build ID `90499bbfde9965ac5092810d27f6e233c53ff0c9` 的真实窗口截图验证了当前布局。此为基础切片：终端 All Notes Off、Program/Bank、Poly Aftertouch、Channel Pressure、SysEx/text/meta、REAPER Data、逐单元格编辑和原厂键盘导航、列操作、准确 loop/source-offset BBT 与跨窗口/重启的模式记忆仍未实现；不计为完整 Event List。

AAADAW Nextest **775/775，0 skipped**（Xvfb）通过；严格 all-target Clippy (`-D warnings`)、`cargo +stable fmt --all -- --check`、`git diff --check` 通过。parity matrix 为 **110 项 / 15 列 / 唯一 ID**，本切片 fixture manifest **4/4 个文件 SHA-256 匹配**。`MIDI-EVENT-LIST-001`、UI-019 与 P7 保持 `in_progress`。


### 2026-10-18 MIDI Event Properties 单音符编辑

Linux REAPER 7.82 / Default 7 的 MIDI Event List section 32061 action 40004（`Edit: Event properties`，Ctrl+F2）打开所选音符的 Type、Position、Length、Channel、Note、Velocity、Off value 与 OK/Cancel/Apply。隔离配置 ReaScript 读回位置 720、长度 240、通道 3、pitch 60、velocity 91，原厂窗口显示 `1.1.75`、`0.0.25`、`60 C4` 和 off value 0；[原厂截图](../verification/reaper-parity/fixtures/midi-event-properties-native.png)、[AAADAW 实窗截图](../verification/reaper-parity/fixtures/midi-event-properties-aaadaw.png)、探针和 TSV 由 [fixture manifest](../verification/reaper-parity/fixtures/midi-event-properties-ui.tsv) 校验。

AAADAW 为一个选中的 Note 增加 `Edit: Event properties` / Ctrl+F2 和 Event List 的 `Edit event…`。对话框支持绝对 BBT 位置/长度、通道、MIDI pitch（数字或音名）和力度。Apply 使用一个 `EditMidiNotes` 历史事务并保持窗口打开，OK 提交并关闭，Cancel/Escape 丢弃草稿；非法位置、音长或字段不会改动音符。稳定桌面构建 SHA-256 `61adadf9ad69c6480f8de94a40aa3500d0a5d2dc0074e450b7bac48281a7e88f` 的实窗操作包含创建音符、Event List 切换、选择行及打开属性窗口。

AAADAW App Nextest **776/776，0 skipped**（Xvfb）、all-target Clippy (`-D warnings`)、fmt 与 diff 检查通过；MIDI Event List 与 Event Properties 两个 manifest 各 **4/4 个条目 SHA-256 匹配**。parity matrix 为 **111 项 / 15 列 / 唯一 ID**。仅支持单音符；CC/Pitch Bend/多选属性、相对值语法、持久化 note-off velocity、跨 meter-map/source-loop 的 BBT 和精确焦点/键盘导航继续开放，`MIDI-EVENT-PROPERTIES-001`、`MIDI-EVENT-LIST-001`、UI-019 与 P7 维持 `in_progress`。


### 2026-10-18 MIDI Event Properties CC 与 Pitch Bend

继续基于 REAPER 7.82 / Default 7 的隔离 profile 测量 action 40004。CC 原厂窗口为 Position、Channel、Controller、Value、CC shape、Bezier tension；样例 CC7 在 tick 480 / channel 3 / value 90 显示 `1.1.50`、`07 Volume MSB`，shape 为 Square、tension 为 0.00。Pitch Bend 原厂 Type 为 Pitch，样例 raw value 9000 同样位于 `1.1.50` / channel 3，并显示相对中心的 `(+808)`。两类都可用 Apply、OK、Cancel；ReaScript 探针、读回 TSV 和裁切原厂截图列在 [Event Properties fixture manifest](../verification/reaper-parity/fixtures/midi-event-properties-ui.tsv)。

AAADAW 的 `Edit event…`、action 40004 / Ctrl+F2 现可打开恰好单选的 Note、CC 或 Pitch Bend。CC 使用命名 Controller 下拉并编辑 0–127 Value；Pitch Bend 编辑 raw 0–16383 Value，并实时显示相对 8192 中心的偏移。位置仍接受绝对 bar.beat.hundredths，通道为 1–16。Apply 对 CC 与 Pitch Bend 分别提交一个现有 `SetMidiControllers` / `SetMidiPitchBends` 历史事务并保持窗口，OK 提交后关闭；非法范围拒绝修改。Note、CC、Pitch Bend 各有 App 回归覆盖；两类表达事件的实窗图额外记录了 Apply、Undo、Redo。

最终普通 Linux 桌面二进制 SHA-256 `735e6339962f2faede8cc56612fe85bf99bd357c9a171a6ec3dbe61a5fd3ded1`。当前 AAADAW 窗口截图分别记录 CC 下拉和 Pitch Bend 14-bit Value；真实输入把 CC 90→95、Pitch Bend 9000→10000 后 Apply，关闭后 Ctrl+Z / Ctrl+Shift+Z 还原与重做均在 Event List 实际显示正确值。AAADAW 包级 Nextest **778/778，0 skipped**（6 个测试二进制）、严格 all-target Clippy (`-D warnings`)、fmt、diff 检查通过；Event Properties manifest **18/18 文件哈希一致**，parity matrix **111 项 / 15 列 / 唯一 ID**。

原生 CC shape / Bezier tension 仍未建模，Note off velocity 仍未持久化；多事件编辑、相对值快捷语法、meter map/source offset/loop BBT 与精确原厂键盘焦点继续开放。因此 `MIDI-EVENT-PROPERTIES-001`、`MIDI-EVENT-LIST-001`、UI-019 与 P7 仍为 `in_progress`。

### 2026-10-18 MIDI Event Properties 同类多选

Linux REAPER 7.82 / Default 7 的 action 40004 原厂窗口以两个同类已选事件复测 Note、CC、Pitch Bend。各类型的 Position、Channel 与对应的 Length/Note/Controller/Value 在选值不同时显示空白；CC 与 Pitch Bend 的共同 CC shape / Bezier tension 仍显示 `Square` / `0.00`。窗口说明数值字段还接受 `+`、`-`、`*`、`/` 相对输入及 `+1o` 八度语法。三组真实窗口、原始事件选中状态、读数和复用探针见 [Event Properties fixture manifest](../verification/reaper-parity/fixtures/midi-event-properties-ui.tsv)。

AAADAW 现对同一 MIDI item 中一个或多个同类 Note、CC、Pitch Bend 事件启用 Event Properties。共同值正常显示，不同值留空；只编辑单个 Value 会保留每个事件原有的 Position、Channel、Pitch/Controller 等混合字段，并将整组更改纳入一个可撤销事务。Apply、OK、Cancel/Escape 与单事件行为保持一致。新增 App 回归分别验证三类两事件选择、混合字段保持、选择状态更新以及 Undo/Redo。

stable 桌面二进制 SHA-256 `58f0b09970b2fb3fe428611bdcaeca0e688af45ff1b700d37b5646aae21e0f27` 在 Xvfb :313 打开包含两个音符的真实 AAADAW 工程；实操切换 Event List、Ctrl-click 多选、Ctrl+F2、仅修改 Velocity 并 Apply，关闭后 Undo/Redo 均在两行实际显示 96/100 → 110 → 96/100 → 110。初始多选属性窗、Apply 后属性窗、事件行及 Undo/Redo 截图收入 manifest。MIDI Event Properties 定向 Nextest **4/4**、AAADAW App 单元测试二进制 **758/758**、strict all-target Clippy (`-D warnings`)、fmt、diff 检查通过；fixture manifest **30/30 SHA-256 有效**，parity matrix **111 项 / 15 列 / 唯一 ID**。

同类型多选已覆盖；混合事件类型的属性编辑、note-off velocity 持久化、CC shape / Bezier tension、meter-map/source-offset/loop BBT 和原厂精确焦点导航仍未实现。相对值编辑与 `+1o2` 已由下一节补齐；`MIDI-EVENT-PROPERTIES-001`、`MIDI-EVENT-LIST-001`、UI-019 与 P7 继续 `in_progress`。


### 2026-10-18 MIDI Event Properties 相对编辑与最终控件验证

在 REAPER 7.82 / Default 7 中追加原厂相对 BBT 探针：两个 Note 分别位于 `1.1.25` / `1.1.50`（960 PPQ 下 tick 240 / 480），Position 输入 `+0.1.00` 并 Apply 后变为 `1.2.25` / `1.2.50`（tick 1200 / 1440），即各自增加一个四分音符拍长。输入窗、Event List 结果、原厂 relative +10 / +1o2 读数共收入 Event Properties manifest。

AAADAW Event Properties 对 Note、CC、Pitch Bend 的相对数值字段支持 `+`、`-`、`*`、`/`，针对各选中事件的当前值逐项计算；Position/Length 的加减支持 BBT offset，Note pitch 支持 `+1o2`。一批事件仍使用单个 Undo 步骤；超出 MIDI/timeline 范围时整批拒绝。最终 Linux 桌面二进制 SHA-256 `eef64c2bf8fc0099bec151f52eb85cfb409bbe516d5500b6fb275080539f0a6b` 在 Xvfb 中验证了可搜索命名 CC combo：搜索 `10` 显示 `10 Pan MSB`，选择并 Apply 将 CC7 改为 CC10，Ctrl+Z 在 Event List 恢复 CC7；搜索、选项、Apply、Undo 四张最终构建实窗图已登记。

AAADAW App 测试目标 **759/759**、Event Properties 定向 Nextest **5/5**、严格 Clippy (`-D warnings`)、stable fmt 与 `git diff --check` 通过。fixture manifest **55/55 文件 SHA-256 有效**，parity matrix **111 项 / 15 列 / 唯一 ID**。混合类型多选、note-off velocity 持久化、CC shape / Bezier tension、非 4/4 meter-map、source-offset / loop BBT 以及精确原厂焦点导航仍未关闭；`MIDI-EVENT-PROPERTIES-001`、`MIDI-EVENT-LIST-001`、UI-019 与 P7 继续为 `in_progress`。

### 2026-10-18 MIDI Event List 上下键导航

REAPER 7.82 / Default 7 原厂四音符 Event List 探针记录：初始三行多选为 `1,1,1,0`；ArrowDown 首次收敛为当前行 `1,0,0,0`，再次下移为 `0,1,0,0`；Ctrl+A 后 Ctrl+ArrowDown 保留 `1,1,1,1`。连续选择状态 watcher、原厂前后截图和序列记录已加入 [Event List fixture manifest](../verification/reaper-parity/fixtures/midi-event-list-ui.tsv)。

AAADAW 在 Event List 模式现在接受未修饰 ArrowUp/Down，沿当前可见、排序后的事件收敛或移动选择；Ctrl+ArrowUp/Down 移动独立事件游标并保留选择。基于含 Note/CC/Pitch Bend 四行的实际工程，当前构建 SHA-256 `c76c9607ac03339e2ee206814f2377b36f2c49da79c0539b7b2e0d870dfbadf6` 的真实窗口已验证多选保持、Ctrl+Up 后普通 Up 收敛到新游标，以及所选行变化。原厂双击数值会打开完整 Event Properties 窗口，因此逐单元格原位编辑不列作差异；Shift+Arrow 范围选择、横向字段焦点和 Home/End 仍待测量/实现。

AAADAW App Nextest **760/760，0 skipped**（Xvfb），严格 all-target Clippy (`-D warnings`)、stable fmt、`git diff --check` 通过；Event List fixture manifest **10/10 文件 SHA-256 有效**，新增的 **6/6** 个 fixture 哈希已核对，parity matrix **111 项 / 15 列 / 唯一 ID**。未建模 MIDI 事件、混合类型属性、note-off velocity、准确 source-offset/loop BBT、列操作与模式持久化仍开放；`MIDI-EVENT-LIST-001`、UI-019 与 P7 保持 `in_progress`。

### 2026-10-18 MIDI Event List Shift+Arrow 范围选择

Linux REAPER 7.82 / Default 7 在单选第一音符后按 Shift+ArrowDown，ReaScript note-selection watcher 从 `1,0,0,0` 读到 `1,1,0,0`；[原厂截图](../verification/reaper-parity/fixtures/midi-event-list-navigation-native-shift-down.png)与完整前序状态记录在 [Event List manifest](../verification/reaper-parity/fixtures/midi-event-list-ui.tsv)。

AAADAW 的 Shift+Arrow Up/Down 现在从独立锚点沿当前可见行扩展或收缩选择；范围可以跨越 Note、CC、Pitch Bend。最新桌面构建 SHA-256 `27334b0a8b7348aef7e9f7123c1653bcea983084a1ff465efc53c774625cf43d` 的真实 MIDI Event List 截图记录跨三种事件扩展、Shift+Up 收缩及普通 Up 收敛。App 回归检查双音符范围、跨类型范围、收缩后只保留锚点到游标之间事件，以及 Ctrl 导航仍保留多选。

AAADAW App Nextest **760/760，0 skipped**，严格 all-target Clippy (`-D warnings`)、stable fmt 与 `git diff --check` 通过；Event List fixture manifest **14/14 文件 SHA-256 有效**，本次新增/更新证据 **4/4** 哈希匹配，parity matrix **111 项 / 15 列 / 唯一 ID**。Shift+click 范围选择、Home/End、横向字段焦点、未建模事件、混合类型属性、note-off velocity 和 source-offset/loop BBT 仍开放；`MIDI-EVENT-LIST-001`、UI-019 与 P7 保持 `in_progress`。


### 2026-10-18 MIDI Program Change 端到端切片

Linux REAPER 7.82 / Default 7 的隔离 profile 探针创建 channel 3、tick 480、原始 program 41 的 `0xC2` Program Change。原厂 Event List 显示 `PC` / `41`；Action 40004 打开 `Bank/Program Select`，并显示 Bank MSB `0`、Bank LSB `0`、Program number `41`。这确认 REAPER 的 Program number 是零基 0–127。ReaScript、读回 TSV 和原厂 Event List / 属性窗截图均登记在两个 MIDI fixture manifest。

AAADAW 新增独立的 `MidiProgramChangeData` 项目事件，并支持校验、选择、复制/拆分/偏移、历史撤销重做、schema 60 保存迁移、SMF `Cn` 导出、实时播放与 seek 状态追踪、实时输入解码、CLAP 原生 MIDI / IPC，以及 CPAL / Android 输出。调度优先级保证同 tick 的 Bank Select CC0/CC32 先于 Program Change，Program Change 又先于 Note On；事件上限仍在实时计划编译时检查。MIDI Event List 支持 PC 行、现有类型/通道过滤、选择、导航、删除和属性窗。实窗验证将值 41 Apply 为 42，再以 Ctrl+Z / Ctrl+Shift+Z 读回 41 / 42；完整桌面二进制 SHA-256 为 `da548df508caa3e42378e8aedba1af0475e5ef099441caf10b911a0ead246ff3`，ELF Build ID 为 `2dc34a348a9b51c2c41be76b5cb1261833fb37b2`。

定向 Core 与 SMF 测试 **85/85**、Storage 工程往返/迁移测试 **62/62**、Engine 全目标测试 **208/208**、AAADAW 全目标测试 **782/782** 均通过；workspace strict Clippy (`-D warnings`)、stable fmt 与 `git diff --check` 通过。Event List 与 Event Properties fixture manifest 各 **18/18**、**61/61** 项 SHA-256 校验一致；parity matrix **111 行 / 15 列 / ID 唯一**。此切片只覆盖 Program number；原厂 Bank MSB/LSB 编辑、patch-file 选择和 Event List native Parameter 编码仍未实现或验证，Poly Aftertouch、Channel Pressure、SysEx/meta 也仍开放。因此 `MIDI-EVENT-LIST-001`、`MIDI-EVENT-PROPERTIES-001`、UI-019 与 P7 继续 `in_progress`。

### 2026-10-18 MIDI Poly Aftertouch 与 Channel Pressure

Linux REAPER 7.82 / Default 7 隔离 profile 实测 `0xA0` Poly Aftertouch 与 `0xD0` Channel Pressure：Event List 分别显示 `Poly Aftertouch / C4 / 37` 和 `Channel Pressure / 74`；action 40004 的 Poly 属性窗显示 Note `60 C4`、Channel `3`、Value `37`，Channel Pressure 属性窗只有 Channel 与 Value，并且两者仍显示未建模的 CC shape / Bezier tension 控件。ReaScript 读回、原厂 Event List 与两个属性窗截图，以及可复现探针均收录在 [aftertouch fixture manifest](../verification/reaper-parity/fixtures/midi-aftertouch-ui.tsv)。

AAADAW 将两个事件贯通到核心 MIDI 项目模型、快照、事务历史、复制/拆分/移动、SQLite schema 61 迁移、SMF `0xA0` / `0xD0` 导出、实时事件排程与 seek chase、实时 MIDI 输入解码、CLAP 原生 MIDI / IPC、CPAL 与 Android 输出。Event List 支持可见行、通道和类型过滤、选择/删除/导航及同类型属性编辑。Properties 支持 BBT position、通道、Poly note/value 与 Channel Pressure value，沿用 Apply、OK、Cancel、边界拒绝及 Undo/Redo。真实 Xvfb 窗口验证了 Alt+3、原厂 `C4` Event List 格式、Filter Events 面板与七种已建模事件类型菜单、Poly value 37→42 Apply、Ctrl+Z / Ctrl+Y 恢复 37/42，以及 Channel Pressure 的 value-only 属性行。最终 stable desktop Build ID `6f55360bf318d4d29f4eb8d245f09c2a0860f36b`，SHA-256 `0e37584eac7ae1593655b9a51e14f1a425e49c1cca862dc5a25ada07f8d20743`。

Core、Storage、Engine、AAADAW 的完整 `--lib --tests` suites 在 Xvfb 下通过；最后的 Event List 渲染调整另经定向测试 **1/1**。Workspace strict all-target Clippy (`-D warnings`)、stable fmt、`git diff --check` 通过；aftertouch manifest **16/16 SHA-256 有效**，parity matrix **111 行 / 15 列 / 111 个唯一 ID**。CC shape / Bezier tension、note-off velocity、native terminal All Notes Off、SysEx/meta、Program Change bank/patch、混合类型属性、非默认 meter-map 与 source-offset/loop BBT 仍开放；`MIDI-CHANNEL-VIEW-001`、`MIDI-EVENT-LIST-001`、`MIDI-EVENT-PROPERTIES-001`、UI-019 与 P7 继续 `in_progress`。

### 2026-10-18 MIDI Note Off velocity 持久化与编辑

Linux REAPER 7.82 / Default 7 的隔离项目通过 `MIDI_SetAllEvts` 写入 channel 3、C4、Note On velocity 91、tick 0 和 Note Off velocity 37、tick 240。ReaScript 读回原始 `0x82` Note Off 数据字节 37；Event List 显示 `Off Value 37`，action 40004 / Ctrl+F2 属性窗显示 `Off value 37`。探针、原始读回和两张 REAPER 实窗图列在 [Note Off velocity fixture manifest](../verification/reaper-parity/fixtures/midi-note-off-velocity-ui.tsv)。

AAADAW 将 Note Off velocity 纳入核心音符数据，校验 0–127 范围，并以 schema 62 迁移和保存旧工程（旧音符默认 0）。Event List 的 Off Value 与属性窗 Off value 都读取该值；Apply、Undo、Redo 实窗操作验证 37→42→37→42。SMF 导出、实时 MIDI 排程和 CLAP Note Off 均传递该 release velocity；MCP 插入、编辑和查询也暴露该字段，未提供编辑值时保留原值。切分跨界音符时，切口处合成的释放事件取 0，保留原始终点的片段继续使用原值。最终稳定桌面二进制 SHA-256 `69fbe80b86314fc88dabb8979283c87348233cb3d405433aca6001d1381fb099`，ELF Build ID `4fc3b0b13df15799b9f4bc748e072d6727ac431f`；当前 Event List、属性窗、Apply、Undo 与 Redo 的实窗图均由此二进制在 Xvfb :4052 捕获，并在上述 fixture manifest 中核验。

Core、Storage、Engine、AAADAW 的完整 Nextest suites **1470/1470，0 skipped**；MCP 边界/写入定向测试 **2/2**；workspace strict all-target Clippy (`-D warnings`)、stable fmt、`git diff --check` 通过。Note Off fixture manifest **12/12 SHA-256 匹配**，parity matrix **111 行 / 15 列 / 111 个唯一 ID**。REAPER 的终端 `(end) All Notes Off` 行、Bank MSB/LSB 与 patch-file、未建模 SysEx/text/meta 和 REAPER Data、混合类型属性、CC shape / Bezier tension、source-offset/loop BBT 以及原厂横向焦点导航仍未完成；`MIDI-EVENT-LIST-001`、`MIDI-EVENT-PROPERTIES-001`、UI-019 与 P7 保持 `in_progress`。


### 2026-10-18 MIDI Event List 合成终点行与 Delete 焦点

Linux REAPER 7.82 / Default 7 空 MIDI item 探针读取到 0 Note、0 CC、0 text event，但 Event List 仍有第 1 行 `(end) All Notes Off`：位置 `3.1.00`、Channel 1、Value 0、Length/Parameter/Off Value 为空。空 item 打开时该行是活动光标；Ctrl+F2 会打开 Type、Position、Channel 均为空的 Event Properties 窗口。原厂 ReaScript 读数、Event List 和空属性窗实截图列在 [Event List fixture manifest](../verification/reaper-parity/fixtures/midi-event-list-ui.tsv)。

AAADAW 现在为 MIDI item 的 `length_ticks()` 添加同位置的合成终点行，空 item 也显示它。该行不进入 MIDI 模型或导出，支持点击、上下键游标导航；Delete/Backspace 被限定在 Event List 删除事件的焦点上下文，不会冒泡成主窗口删除 item 的命令。点击终点行后 Delete 不改变工程；选中真实 Note 后 Delete 只删除该 Note，终点行保留。还修复了方向键导航没有清掉 Poly Aftertouch / Channel Pressure 旧选择的问题。原厂空 item 探针、空/四音符 AAADAW fixture 生成器、空白属性窗、空/非空 Event List、Delete no-op 与真实 Note 删除截图均登记在 manifest。REAPER 与 AAADAW 混合事件窗口复核后，非 Note 行（包括终点行）的 Length 现在留空；Aftertouch fixture 的 Note 长度已修正为原厂样例的 0.0.25，Poly Aftertouch/Channel Pressure 长度留空。

稳定桌面二进制 SHA-256 `b0562fe90b2a72570024fddf50a2f275f3566dbf2e66f3bebad80cb852ec0564`，ELF Build ID `3044a174aeb547f866dc8c55ed43b1f585d30f2e`。MIDI Event List 定向测试 **4/4**；workspace Nextest **1472/1472，0 skipped**；strict all-target Clippy (`-D warnings`)、stable fmt、`git diff --check` 通过。Event List manifest **33/33 SHA-256 有效**，parity matrix **111 行 / 15 列 / 111 个唯一 ID**。Ctrl+F2 终点行的空白属性窗尚未实现；REAPER Data、未建模 SysEx/text/meta、Program Change bank/patch、混合类型属性、原厂排序/列与上下文菜单、精确 source-offset/loop BBT 等仍开放，`MIDI-EVENT-LIST-001`、`MIDI-EVENT-PROPERTIES-001`、UI-019 与 P7 继续 `in_progress`。


### 2026-10-18 MIDI Event List Home/End 光标定位

Linux REAPER 7.82 / Default 7 的隔离探针使用一个 4–8 秒 MIDI Item（首音符位于 item 内 0.25 秒）和一个延伸到 12 秒的后续 item，避免把 MIDI 边界与工程边界混淆。原厂读回显示初始光标 6 秒；Home 选中首条可见 Note 并移到 4.25 秒，End 移到 MIDI Item 末端 8 秒且保留 Note 选择。空 Item 只有合成 `(end) All Notes Off` 行，Home 会将光标放在该行末端。ReaScript、selection/cursor TSV 与两张原厂真实窗口截图已加入 Event List fixture manifest。

AAADAW 的 Event List 现在以 Home 选中筛选后首条可见事件（包括空 Item 终点行），并将 MIDI 与工程编辑光标移到该事件位置；End 将两种光标移到 Item 末端，同时保留当前事件选择。应用窗口的 Home/End 截图显示首音符行保持选择，主工程读数分别为 `1.1.480 / 0:00.250` 与 `2.1.00 / 0:02.000`。桌面二进制 SHA-256 `a1a6bb936d43fc4219e511572d4533ded013c3204ec0bd2e4c0de6d1d5ba6f21`，ELF Build ID `8b355d36dc6e34b6e2005cc99083f13e63d46095`。

Home/End 与空 Item 定向 App 测试 **2/2** 通过，包含首事件选择、End 保留选择、空 Item 合成行以及导航不修改工程快照/版本。workspace Nextest、严格 all-target Clippy、fixture manifest 哈希校验随后运行；`MIDI-EVENT-LIST-001`、UI-019 和 P7 继续 `in_progress`。REAPER Data、SysEx/text/meta、混合类型属性、排序/列操作、Shift+click 和 source-offset/loop 显示仍开放。


### 2026-10-18 Standard MIDI File 导入基础切片

AAADAW 新增 `Insert > MIDI file…`。文件在后台线程解析；SMF Type 0/1 的每条含通道事件的源轨会在编辑光标处创建一个命名轨道和一个非循环 MIDI item，整份文件通过单个 Project transaction 导入，可一次 Undo/Redo。解析器校验 chunk 边界、VLQ、状态/数据字节、End of Track 与资源上限，展开 running status，并在解码结果中保留原始 SysEx 和 Meta 事件。

当前工程转换支持 Note、CC、Pitch Bend、Program Change、Poly Aftertouch 与 Channel Pressure，按源/工程 PPQ 四舍五入缩放，重复同音事件按 FIFO 配对。畸形文件、未配对 Note On/Off 或零时值音符会拒绝整次导入。Meta-only 源轨不创建空工程轨；SysEx 与未建模 Meta 被计数并在导入状态中告知用户。解析结果可保留原始事件，但工程模型尚不能存储它们；导入不会应用文件 Tempo/Meter，也不保留跨事件类别的同 tick 原始顺序。

SMF parser/import Core 定向测试 **7/7**、AAADAW import 消息处理 **1/1**、SQLite imported-track 保存重开 **1/1** 通过；项目快照恢复、事务回滚、Undo/Redo 均在 Core 测试覆盖。workspace Nextest **1482/1482，0 skipped**（Xvfb）、strict stable all-target Clippy (`-D warnings`)、stable fmt 与 `git diff --check` 通过。文件解码并不等同于 REAPER 行为核验：当前 stable 桌面 Build ID `dda5cda44978777bf649ddce7efb9f2c71b5a6a0` 的 [Action List 截图](../verification/reaper-parity/fixtures/midi-file-import-aaadaw-action-list.png)只确认 `Insert > MIDI file…` 入口；文件选择器及导入后 Arrangement 截图未验证，Linux REAPER 7.82 / Default 7 的导入语义仍待探测。`MIDI-FILE-IMPORT-001`、FILE-MIDI-EXPORT-001、UI-019 与 P7 仍保持 `in_progress`；SysEx、文本/Meta 持久化、格式 2、SMPTE time division 与 native A/B 仍是未完成项。 JACK/PipeWire feature 编译检查已尝试；当前镜像缺少 `jack.pc` 与 JACK 开发头文件，因而未能完成该环境检查。

### 2026-10-18 MIDI Bank Select 与 Program Change 属性

REAPER 7.82 / Default 7 的既有 Bank/Program Select 探针显示独立 Bank MSB/LSB 数字字段，默认均为 0，Program number 使用 0–127 的零基值；原厂窗口与 Event List 读回记录见 [Event Properties fixture manifest](../verification/reaper-parity/fixtures/midi-event-properties-ui.tsv)。

AAADAW 的 Program Change 属性窗现在从该事件 channel 上不晚于其 tick 的 CC0/CC32 读取生效银行值；没有前置选择时显示 0。编辑 Bank MSB/LSB 会在修改后的 Program Change channel/tick 创建对应 controller，或替换同位置同控制器的既有点；Program Change、CC0、CC32 放入同一 BatchTransaction，保持一次 Undo/Redo。银行字段未触碰时只编辑 Program Change，不会隐式插入零值 Bank Select。超出 0–127 的银行值拒绝且保留窗口草稿。

`midi_bank_program_properties_edit_cc0_and_cc32_atomically_with_program` 覆盖继承银行、同 tick 替换、PC+银行单步历史、Undo/Redo 与非法值；AAADAW 的 `midi_` 定向套件 **174/174**、引擎 CC0/CC32/PC 同 tick 排序 **1/1**、`audio-device` 播放图失效回归 **1/1** 均通过；strict Clippy (`-D warnings`) 覆盖 AAADAW 与 Engine 的 all-target/audio-device。当前稳定 Linux 桌面二进制 SHA-256 `02670e0c764d88dcafb18732194ca110cfcc8a49ab63fbe5e600a3968f8daf54`，ELF Build ID `2dd542d14a9fa54d58cccd95373cc23f96cd441d`；Xvfb 实窗截图显示 Event List 选中 PC 后的 Bank MSB (CC0)=0 / Bank LSB (CC32)=0，见上述 manifest。REAPER 原厂 Bank 对话框截图已用于标签/初始值对照；本轮又完成银行值修改后的 native Event List 读回，patch-file 载入、混合事件类型属性和完整视图对齐仍未完成。`MIDI-EVENT-PROPERTIES-001`、UI-019 与 P7 保持 `in_progress`。


本轮 REAPER 交互探针将 Bank MSB/LSB/Program 从 `11/22/41` 改为 `12/23/42` 并确认；原始 MIDI 读回为 channel 3、tick 480 的 CC0=`12`、CC32=`23`、PC=`42`。原厂 Event List 把同位置同 channel 的 CC0/CC32 合并成 `Bank Select 14-bit (0/32)`，值 `1559 = 12×128+23`，PC 参数显示 `12-23-42`。前后事件、编辑前后对话框、结果列表截图及 ReaScript 已登记在 Event Properties / Event List fixture manifests。AAADAW Event List 已完成 Bank Select 合并行和 PC 银行三元组显示；`MIDI-EVENT-LIST-001`、`MIDI-EVENT-PROPERTIES-001`、UI-019、P7 均保持 `in_progress`。


### 2026-10-18 MIDI Event List Bank Select 对齐

根据上述原厂对照，Event List 将同 tick、同 channel 的 CC0/CC32 折叠为一个 `Bank Select 14-bit (0/32)` 行，值为 `(MSB << 7) | LSB`；Bank 行选择、导航范围和删除将一并操作两条底层 controller，Delete/Undo 实窗已核对。Program Change 参数列显示该 channel/tick 的生效 `MSB-LSB-Program`，Value 仍是零基 Program；所有非 Note 行的 Off Value 现在留空。未配对或不同时位置的 CC0/CC32 保持普通 CC 行。

Event List 单元覆盖成对/未配对事件、通道/位置精确配对、14-bit 值、PC 默认与已继承 bank 标签；App 回归验证选择、双事件删除和 Undo。AAADAW `midi_` 定向套件 **176/176**，Bank Select / Program Change 定向测试 **2/2**，all-target `audio-device` strict Clippy 与 stable fmt/diff 通过。AAADAW app suite **769/769** 通过；stable Linux Xvfb 实窗的初始/选中/删除/撤销截图和可复用工程生成器登记在 [case manifest](../verification/reaper-parity/fixtures/midi-event-list-bank-program-aaadaw.tsv)。最终桌面 Build ID `9122c7c08d79277bec523d8dce8ca05db4c4b7ab`，SHA-256 `c8085ee87041f73c7aa7013870b3b610a1b22a55f707497e92acb1d2969e65b0`。`MIDI-EVENT-LIST-001`、UI-019 与 P7 仍 `in_progress`，其余列、混合事件属性、Meta/SysEx、patch-file 与 source-offset/loop 行为仍需实现和原厂验收。

### 2026-10-18 Linux REAPER SMF 导入行为对照

使用 960 PPQ 的 Type 1 确定性文件，分别按 REAPER 7.82 / Default 7 的默认设置与三个导入选项全开执行。默认选项均关闭：REAPER 将导体轨、Lead 与 metadata-only 源轨的事件合并进 edit cursor 处一条名为 `Conductor` 的 MIDI item，工程 Tempo Map 与 project marker 数量保持不变。展开轨道、Tempo Map、Marker/Cue 全开后，REAPER 创建 `Lead` 和 `Meta only` 两条 MIDI 轨；在 edit cursor 2 秒处加入 Tempo/Meter 变化，并把两个 MIDI Marker/Cue 转成 project markers。两种设置下，item 内都保留文本、Marker/Cue Meta 与 SysEx。原厂选项窗口、两种结果 Event List 截图、SMF fixture、ReaScript 与原始读回分别在 [MIDI import probe fixtures](fixtures/midi-file-import-native/)；可复现脚本位于 `scripts/reaper_parity/generate-midi-file-import-probe.py` 与 `scripts/reaper_parity/probe-midi-file-import.lua`。

AAADAW 现有导入仍会按源轨分轨、跳过 metadata-only 轨并丢弃未建模事件，未提供原厂三项选择，也没有导入 Tempo/Meter 和 project marker。因此 `MIDI-FILE-IMPORT-001` 仍保持 `in_progress`；接下来先扩充可持久化的 item 事件和选项化事务导入，再验证桌面实窗与保存重开。原厂文件、读回和截图见 [MIDI import probe fixtures](fixtures/midi-file-import-native/)。

### 2026-10-18 MIDI 导入选项、Meta/SysEx 持久化与实窗验收

AAADAW 的 `Insert > MIDI file…` 已提供 REAPER 对照确认的三项导入选项，默认全部关闭：合并/展开源轨、应用 Tempo/Meter、将 Marker/Cue 转成工程标记。导入事务会在确认后一次性写入轨道、item 事件、节奏/拍号和标记；取消保持工程不变。SQLite schema 63 持久化 opaque MIDI Meta/SysEx，Event List 可显示并定位这些只读行。Meta/SysEx 原样导出；Tempo/Meter 变更遇到非小节线位置时按 REAPER 行为向后吸附到下一有效小节线。

在 Linux Xvfb 实窗运行当前桌面 Build ID `2afc7079777827714f5e174ba72c3cf2a6e1bdd1`，通过隔离的 D-Bus FileChooser 返回仓库中的原厂探针 MIDI 文件，实际打开导入选项、全选三项并完成导入。实窗结果为 Lead 与 Meta only 两条轨、两个工程标记，以及导入点的 150 BPM / 3/4；对话框默认状态、导入后 Arrangement 和 Tempo/Meter 状态截图连同二进制/fixture 哈希登记于 [AAADAW live import manifest](../verification/reaper-parity/fixtures/midi-file-import-aaadaw.tsv)。该截图验证覆盖 AAADAW 导入 UI 到结果，不替代原厂 REAPER 读回；原厂语义截图继续保存在 [native fixture](fixtures/midi-file-import-native/)。

验证：AAADAW 二进制测试 **770/770**、SMF Core import/export **92/92**、Storage project-store **67/67** 通过；完整 workspace Nextest **1490/1490，0 skipped** 在本轮 off-bar meter snapping 改动前通过。稳定 `cargo fmt --all` 与 `git diff --check` 通过。实现仍未完成：Type 1 展开会漏掉首轨通道事件；关闭 Tempo Map 时源 tempo/meter Meta 被丢弃；opaque 行尚不可编辑、删除或筛选；音频播放不调度 Meta/SysEx；不同事件类别的同 tick 次序未完整保留；Format 2 与 SMPTE division 不支持。故 `MIDI-FILE-IMPORT-001`、UI-019 与 P7 仍为 `in_progress`，不会按已完成登记。

### 2026-10-18 Type 1 首轨通道事件导入保护

修正展开 Type 1 MIDI 时无条件跳过首轨的问题：只有首轨不含 channel events 时才按导体轨跳过；如果首轨含音符或控制事件，它会和其余源轨一起导入，避免静默丢失内容。新增 Core 回归覆盖首轨/第二轨各含一个已配对音符、展开后两轨与音符均保留。MIDI file import 定向套件 **9/9** 通过；首轨含通道数据时 REAPER 原厂的精确处理尚未探测，因此 parity 文档明确保留此项为待核实边缘语义，而不是宣称完全对齐。

### 2026-10-18 Event List opaque Meta/SysEx 删除

MIDI Event List 的 opaque Meta/SysEx 行现进入 Ctrl/Command 多选与 Shift+Up/Down 范围选择；`Delete selected` 与 Delete/Backspace 通过 Project `SetMidiOpaqueEvents` 历史事件批量删除选中行，Undo/Redo 原样恢复 tick/order/kind/payload。底栏提示 Ctrl/Cmd-click 与 Shift+Up/Down 选择方式，混合 opaque/可编辑选择会禁用事件属性编辑。app 回归覆盖两条不同类型 opaque 行的 Ctrl 多选、Shift 范围选择、删除、Undo 与 Redo。Linux Xvfb 实窗加载 Type 1 原厂探针文件：初始 Event List 12 项，Ctrl-click 选中 `Unattached text` 与 `Verse text` 后删除为 10 项，Ctrl+Z 恢复两行和 12 项计数；选中/删除/撤销截图及 Build ID `b2ae430d8c00c41651856f695d938076ea92ee39` 在 [opaque-event delete manifest](../verification/reaper-parity/fixtures/midi-opaque-event-delete-aaadaw.tsv)。

同时重新核对 Default 7 默认导入 Event List 原厂截图：Tempo Map 未勾选时工程 tempo marker 数为 0，合并 item 中也无 tempo/meter Meta 行；AAADAW 默认模式一致。此前将此行为登记为差异的描述已修正；expanded tracks 且关闭 Tempo Map 的组合仍未独立探测。AAADAW app 测试 **771/771**、SMF Core import/export **93/93**、workspace strict all-target Clippy (`-D warnings`)、fmt/diff 检查通过。opaque event 编辑、Event List 类型筛选、SysEx/Meta 播放、完整跨类型同 tick 排序、Format 2 与 SMPTE time division 仍开放；`MIDI-FILE-IMPORT-001`、UI-019、P7 保持 `in_progress`。

### 2026-10-18 Event List Sysex/Meta 类型过滤

Linux REAPER 7.82 / Default 7 原厂 Filter Events 窗口确认事件类型列表及顺序为 `<all events>`、Note、Poly Aftertouch、Control Change (CC)、Program Change (PC)、Channel Pressure、Pitch、Sysex/Meta。原厂将 SysEx 与所有 Meta 合并为同一筛选项。导入 Type 1 探针后启用 Sysex/Meta 过滤，REAPER 显示 Track name、Text、Marker、SysEx 行，隐藏 Note/CC/PC 与合成 `(end)` 行；反转后显示 Note/CC/PC 以及合成 `(end) All Notes Off`，隐藏全部 Meta/SysEx。原厂选项、正向筛选和反向筛选实窗截图已登记在 [Event List filter manifest](../verification/reaper-parity/fixtures/midi-event-filter-ui.tsv)。

AAADAW 的事件类型选项现在按原厂顺序和标签显示，opaque Meta/SysEx 使用合并 `Sysex/Meta` 类型；过滤后的列表和键盘导航候选共享同一事件序列，正向过滤隐藏合成终点行，反向过滤则显示它。相同 Type 1 MIDI 文件的 AAADAW 当前稳定桌面构建实窗截图确认选项、8 条 Sysex/Meta 行以及反转后的 4 条 Note/Bank Select/PC/(end) 行，与原厂筛选结果一致。AAADAW 二进制 SHA-256 `71b66e18941140a439ffaf6d39a986182469d627184281734a4474caa5a03ced`，ELF Build ID `6303654aef28033a94eeddb284b99239a55f426d`；native REAPER SHA-256 `9976f5608068b7a58518969001b00b527fb0de6ed7459e0e5dda6f29878ee254`，Build ID `fb9ade2f211bc91a3ee6193630ef17bcf5f6528b`。两端截图哈希均记录于上述 manifest。

AAADAW 全量 App 测试 **771/771**、workspace strict all-target Clippy (`-D warnings`)、stable fmt 与 diff 检查通过。原厂实测目前只覆盖 Sysex/Meta 正向/反向筛选；其他类型组合、通道 mask 交互、Parameter/Value/Position/Repeat/Length、Solo 和 Set/Add/Remove selection 控件仍待实现或探测；opaque 事件属性编辑、REAPER Data、SysEx/Meta 播放、其余排序/列交互及 source-offset/loop 显示仍开放。`MIDI-EVENT-LIST-001`、UI-019、P7 继续保持 `in_progress`。

### 2026-10-18 Event Properties 的 Meta/SysEx 编辑入口与文件动作

Event List 的 `Edit event…` 和原厂 `Ctrl+F2` action 40004 现在可打开一个 text Meta 或 SysEx 行的属性窗。实现覆盖 REAPER 的 Text event、Copyright notice、Track name、Instrument name、Lyrics、Marker、Cue、Program name、Device name 类型，以及文本、类型、BBT Position、多行编辑和 SysEx 文件包络逻辑；两种事件都由一个 `SetMidiOpaqueEvents` 历史动作提交，非法十六进制和非法 BBT 保持属性窗打开。新增 App 测试覆盖 Meta 编辑/Undo/Redo、文件载入后的多行状态、SysEx 非法十六进制及 Undo/Redo、F0/F7 文件包络与带换行 Meta 的 Event List 显示。

Linux REAPER 7.82 / Default 7 的 Type 1 探针实窗确认 SysEx 的 F0/F7、十六进制区、Position、Save/Load，以及 Meta 的 Text、9 个 Type 选项、Multi-line text、Position、Load、OK、Cancel。当前 AAADAW 稳定桌面二进制 SHA-256 为 `10c992915ca62188a73d51df1398534466859bc57bc5c74e56c6c9320ca759e1`，ELF Build ID `98091bbc9cf7d861cd9ddde49f75d677c3d2e663`；原厂二进制身份与当前构建截图列在 [Event Properties fixture manifest](../verification/reaper-parity/fixtures/midi-event-properties-ui.tsv)。Xvfb :455 的 Type 1 实窗通过桌面 portal 加载两行 UTF-8 文本，确认自动切换 Multi-line、选取 Marker 并将两行文字提交至 Event List；另将 SysEx 从 `43 12` 改为 `43 12 34`，Event List 显示 `F0 43 12 34 F7`，Undo 恢复 `F0 43 12 F7`。

六项聚焦 App 测试通过：五项匹配 `opaque_` 的属性/列表测试与一项 F0/F7 文件包络测试；strict all-target Clippy (`-D warnings`)、stable fmt、diff 检查和稳定桌面构建均通过。首次测试链接曾因本地 GLib 搜索路径配置不足失败；补充本地 `LIBRARY_PATH` 后测试二进制成功链接并通过。`MIDI-EVENT-PROPERTIES-001`、`MIDI-EVENT-LIST-001`、`MIDI-FILE-IMPORT-001`、UI-019 与 P7 仍是 `in_progress`。剩余边界包括 SysEx `.syx/.sysex` 文件读写、invalid UTF-8 Meta、F7 continuation 属性编辑、结构性 Meta 属性、mixed-type 选择、REAPER Data、patch-file selection 与 BBT meter/source-offset/loop 语义。

### 2026-10-18 修正 F7 SysEx 延续事件属性显示

SysEx Event Properties 现在区分 SMF 的 F0 消息和 F7 延续事件：F0 显示完整消息边界，F7 显示延续状态且不附加人为的 F0/F7 外框。App 回归验证 F7 payload 编辑后仍保持 F7 状态，并可 Undo/Redo；视图测试覆盖两类边界标签。该切片目前由自动化验证，F7 对应的 REAPER 原厂窗口行为与截图还未 A/B 确认。

当前 `opaque_` 定向 App 测试 6/6、F0/F7 文件封装测试 1/1、F0/F7 属性边界视图测试 1/1 通过；AAADAW 构建、strict all-target Clippy (`-D warnings`)、fmt 与 diff 检查通过。矩阵仍将原厂 F7 对照、SysEx 文件对话框 I/O、invalid UTF-8 与结构性 Meta 留在开放差异，`MIDI-EVENT-PROPERTIES-001` 保持 `in_progress`。

### 2026-10-18 非文本 Meta 原始数据编辑

Event List 现在为结构性/未知 Meta 行显示对应类型名称与原始十六进制字节。选中事件可打开只读的 Meta 类型标识和可编辑 raw-hex payload；非法 hex 不提交，合法修改保留 Meta type、同一事件顺序及 Undo/Redo。Key signature、MIDI channel prefix、tempo、time signature 等 type name 已能识别，但常规导入中被映射到工程 Tempo/Meter 的有效事件仍按原有路径处理。此功能只提供安全的原始字节编辑，不等同于 REAPER 解码后的特定结构字段窗。

关键签名 Meta 的 App 回归覆盖属性入口、错误输入保护、类型保留和 Undo/Redo；Event List 视图回归覆盖类型名与原始 bytes。当前 opaque 属性 App 测试 7/7、结构性 Meta 定向 App/视图测试 2/2、SysEx 包络与 F0/F7 标签测试各 1/1 通过。当前 AAADAW Build ID `977623f4e8716be4b47b4d0c2c80be5fb721e3cd` 在 Xvfb :456 实际打开 Key signature 属性窗；520×240 截图和哈希记录在 [Event Properties manifest](../verification/reaper-parity/fixtures/midi-event-properties-ui.tsv)。REAPER 对非文本 Meta 的具体属性语义尚未 A/B 探测；后续验证 SysEx 文件对话框 I/O，再按原厂结果补结构化 Meta 行为。`MIDI-EVENT-PROPERTIES-001` 保持 `in_progress`。

### 2026-10-18 SysEx `.syx` 文件读写实测

将 SysEx 文件读写逻辑抽为可直接验证的路径函数，并为 F0 完整消息、F7 延续包增加真实文件系统往返测试。当前 Linux 桌面构建在 Xvfb :456 通过受控 portal 从 `.syx` 文件载入 `F0 43 12 34 F7`，属性窗显示 payload `43 12 34`；保存后逐字节读回 `F0 43 12 34 F7`。现场截图展示 F0/F7 边界、已载入 payload 和 `SysEx data saved` 状态，SHA-256 与构建身份记录在 [Event Properties fixture manifest](../verification/reaper-parity/fixtures/midi-event-properties-ui.tsv)。

三项 `midi_sysex` 定向 Nextest 测试全部通过，覆盖 F0/F7 封装函数与两类文件读写往返；`cargo build`、strict all-target Clippy、stable fmt 和 diff 检查此前已针对同一代码通过。F7 文件字节保留目前由自动化往返验证，尚无原厂 F7 对照；`.sysex` 文件选择及原厂文件对话框外观/行为也待 A/B。`MIDI-EVENT-PROPERTIES-001` 与 P7 继续保持 `in_progress`。

### 2026-10-18 对齐 F7 SysEx 延续包导入

Linux REAPER 7.82 / Default 7 的原厂 Type 0 MIDI 探针输入 F0 起始包 `43 12`（tick 240）和 F7 延续包 `34 F7`（tick 480）。REAPER 把两包合成一个 F0 事件 `F0 43 12 34 F7`，位置移至最终包 tick 480；事件属性显示 payload `43 12 34`、Position `1.1.50`。原厂读回、探针 MIDI 和窗口截图在 [Event Properties fixture manifest](../verification/reaper-parity/fixtures/midi-event-properties-ui.tsv)。

AAADAW 的 SMF 解码层仍保留源包，项目导入转换则合并仍开放的 F0/F7 连续包并使用最后一包位置；未匹配 F7 仍保留原始状态。当前桌面构建通过受控 portal 导入同一 fixture，Event List 单行参数与属性窗 payload 均和 REAPER 一致；两张实窗截图及二进制身份记录在同一 manifest。`midi_file_import` Nextest 11/11、strict core all-target Clippy、stable fmt 与 diff 检查通过。REAPER 未匹配 F7 escape 行和跨其他事件间隔的延续规则仍需原厂探测；相关 parity 项保持 `in_progress`。

### 2026-10-18 补齐 F7 SysEx 导入边界

七个 Linux REAPER 7.82 / Default 7 Type 0 MIDI 探针补全了前一轮尚未核实的导入边界。`MIDI_GetAllEvts` 读回表明：孤立 F7 不生成事件；仅当 F7 紧邻仍未结束的 F0 时才拼接；轨尾、插入非 SysEx 事件或开始另一个 F0 时，REAPER 会在原 F0 数据末尾补 F7 并关闭它；没有终止字节的 F7 continuation 会继续拼接，直到链关闭。七个输入、原厂读回和 SHA-256 见 [Event Properties fixture manifest](../verification/reaper-parity/fixtures/midi-event-properties-ui.tsv)。

AAADAW 项目导入现保留解码层的原始包边界，并按上述规则归一化项目事件：合并紧邻的延续包、将事件移到最终包位置、补全 F0 结束字节、遇到新事件/新 F0 时结束旧消息，并丢弃未匹配 F7。Core 导入测试 13/13 通过，覆盖轨尾、CC/Meta 打断、新 F0、无终止字节 continuation 和孤立 F7。MIDI Event Properties 对直接存储的 F7 事件仍保持原始状态编辑行为；文件导入归一化不改变这一路径。`MIDI-FILE-IMPORT-001`、`MIDI-EVENT-PROPERTIES-001` 与 P7 保持 `in_progress`。

当前 Cranelift nightly Linux 桌面构建通过受控 portal 导入 F0@240 + Text Meta@360 + F7@480 fixture；实窗 Event List 显示 Track Name、`F0 43 12 F7` SysEx、Text Meta 和结束行，没有独立 F7 行。截图、Build ID、SHA-256 与 fixture 哈希见同一 manifest。AAADAW desktop Cranelift build、Core 定向 Nextest、strict Core Clippy、stable fmt 与 diff check 均通过。

另一个 REAPER 原厂探针在新 MIDI take 上通过 `MIDI_SetAllEvts` 写入选中 raw event `F7 34 F7`，再排序并调用 `MIDI_GetAllEvts`。读回只剩终止 All Notes Off，Event List 实窗也只有结束行；Action 40004 打开的属性表为空。readback、Event List 和空属性窗截图已登记。REAPER 在该项目 API 路径不保存独立 F7 行；AAADAW 仍保留对直接/旧项目 F7 状态的属性编辑以兼容已有数据，此行为不作为原厂文件导入 parity 通过项。

### 2026-10-18 MIDI 同 tick 跨类别顺序基线

两份反向排序的 Type 0 fixture 在 tick 240 各包含 SysEx、Text Meta、Pitch Bend、CC、Program Change、Poly Aftertouch、Channel Pressure 和 Note On。Linux REAPER 7.82 / Default 7 的 `MIDI_GetAllEvts` 两次读回均显示：Meta/SysEx 在通道事件前，Program Change 在其他通道事件前，剩余非音符通道事件沿用源顺序，Note On 最后。输入、Lua 探针、读回和文件 SHA-256 记录在 [same-tick order fixture manifest](../verification/reaper-parity/fixtures/midi-file-import-same-tick-order.tsv)。

AAADAW 为导入的 Note、CC、Pitch Bend、Program Change、Poly Aftertouch 和 Channel Pressure 保存源序号；Event List 对这类带有原厂顺序元数据的项目按 Meta/SysEx、Program Change、其他通道事件、Note 的原厂分组并在组内保序。没有导入顺序元数据的旧项目和手工事件继续使用原 Event List 类别顺序，避免改变既有 Bank Select 等行为。快照保存/恢复携带元数据，SQLite schema 64 新增 MIDI event-order 表，schema 63→64 migration 和数据库重开回归通过。Core 导入测试 15/15、UI 正反向排序测试 1/1、导入后音符/CC 编辑删除并重开快照测试通过、Storage project-store 68/68、AAADAW desktop 782/782、strict all-target Clippy (`-D warnings`)、fmt 和 diff 检查通过。

从最终 nightly Cranelift 桌面二进制在独立 Xvfb 会话中分别导入正向、反向 fixture；两张实窗 Event List 截图与 REAPER 两组 `MIDI_GetAllEvts` 读回和原厂截图顺序一致。最终 Build ID `bedf723e6e760b01721e3b47f4e5ba53fca4e4bc`、binary SHA-256 `95a36c1901bb044d07ea45cf0f42657124bd28cfaf758bb0b6bc3ab1310f5ae2` 及两张截图 hash 记于 [same-tick order fixture manifest](../verification/reaper-parity/fixtures/midi-file-import-same-tick-order.tsv)。`MIDI-FILE-IMPORT-001` 继续保持 `in_progress`；原厂 Note Off 同 tick 的 SMF 再导出顺序还未核验，SysEx/Meta 播放与 `.sysex` 选择器仍未完成。

### 2026-10-18 保留导入 MIDI 的 Note Off 源顺序并对照原厂导出

REAPER 7.82 / Default 7 的 Type 1 导出探针确认：导入同一 Type 0 fixture 后，tick 480 的事件顺序为 CC7、Note On、Note Off。AAADAW 之前将 Note Off 固定排在其他事件之前；现在导入模型同时保存 Note On 与配对 Note Off 的源序号，SMF exporter 对导入事件按原厂 Meta/SysEx、Program Change、其他通道事件、音符分组并在组内保序。新增 SQLite schema 65 保存 Note Off 序号，schema 64 及更早版本迁移均可通过；旧版快照没有该字段时仍按兼容默认处理。

原厂 Type 1 MIDI 输出、导入 take 的 `MIDI_GetAllEvts` 顺序、导出设置和 REAPER 实际写出提示截图记录在 [same-tick Note Off export manifest](../verification/reaper-parity/fixtures/midi-file-export-same-tick-note-off.tsv)。回归测试将 AAADAW 导出与原厂文件在 tick 240 和 tick 480 的全部事件逐项对比。Core 导入、导出和存储集成测试 169/169，opaque-only Event List 顺序定向 App 测试 1/1，Core/Storage/App strict all-target Clippy (`-D warnings`)、fmt 和 diff 检查通过。

同时修复只有 Meta/SysEx、没有 typed-channel 顺序记录的导入 item：Event List 现在仍按这些 opaque 行的源序号排序。MIDI 文件格式、SysEx/Meta 播放、文件选择器等其它差异尚未完成，`MIDI-FILE-IMPORT-001` 与 P7 继续保持 `in_progress`。

### 2026-10-18 音符编辑后延续导出顺序元数据

后续审查发现，导入音符的音高、位置、时长或力度一旦被编辑，事件顺序目标仍指向旧数据，导出会退回到缺省排序。普通音符编辑、Quantize 和 Humanize 现在会同步更新对应 Note 的顺序目标，并保留 Note On 与 Note Off 的源序号；撤销通过反向事件将目标恢复。新增回归用例将导入音符改音高后重新导出，并把 tick 240、480 的事件序列与 REAPER 原厂结果逐项比较。

CC、Pitch Bend、Program Change、Poly Aftertouch 与 Channel Pressure 的值编辑也会同步更新类型化顺序目标。Core 测试覆盖这些编辑、重复音符源序号、普通音符 Undo/Redo，以及 Quantize/Humanize 后的元数据同步与 Undo/Redo；导入/导出和 Storage 集成套件 173/173 通过。Core/Storage/App strict all-target Clippy (`-D warnings`)、fmt 和 diff 检查通过。`MIDI-FILE-IMPORT-001` 与 P7 继续保持 `in_progress`。

### 2026-10-18 支持 SMF Format 2 独立序列导入

Linux REAPER 7.82 / Default 7 原厂探针对包含两个独立序列的 Format 2 / 960 PPQ 文件验证默认与展开路径：默认把两序列合并到名为 `Sequence A` 的一个 item，二者均从 tick 0 开始；勾选展开后生成 `Sequence A` 与 `Sequence B` 两条轨道/两个 item，各自只保留对应序列。该格式的 Tempo/Meter 与 Marker/Cue 项目导入控件禁用。输入 fixture、选项及结果截图、两条路径的 `MIDI_GetAllEvts`/note 读回和哈希见 [Format 2 manifest](../verification/reaper-parity/fixtures/midi-file-import-format-2.tsv)。

AAADAW 现在解码 Format 2，将序列保留为独立 source tracks，默认按 REAPER 规则合并，启用 Expand 时逐序列插入；项目 Tempo/Meter 和 Marker/Cue 控件与应用路径均对 Type 2 禁用。Format 2 用例扩展 `midi_file_import` 到 19/19，Core 导入/导出与 Storage 集成测试 174/174、AAADAW 全套 805/805 通过；Core/App strict all-target Clippy (`-D warnings`)、stable fmt 和 diff 检查通过；`MIDI-FILE-IMPORT-001` 与 P7 保持 `in_progress`。

### 2026-10-18 对齐 REAPER 的高位 MIDI division 导入行为

REAPER 7.82 的 `MIDI_GetProjTimeFromPPQPos` 实测表明，SMF header division `0xE728` 被当作无符号 PPQ 59176，而不是其标准定义的 -25 fps / 40 ticks-per-frame。Source tick 250/750 分别读为 0.0021123428/0.0063370285 秒；`MIDI_GetPPQPosFromProjTime(0.25)` 得到 29588。AAADAW 现在同样以 59176 PPQ 缩放事件；在项目 960 PPQ 下音符起止落在 tick 4/12。原厂 MIDI item 为默认 120 BPM 下的 0.5 秒，即使最后源事件不足 0.01 秒；AAADAW 导入 item 现保持至少一个四分音符。Lua 探针、输入、REAPER 窗口截图、readback 和哈希见 [SMPTE division fixture manifest](../verification/reaper-parity/fixtures/midi-file-import-smpte.tsv)。

高位 division 和 one-quarter-note item floor 的 Core 回归包含在本次 `midi_file_import` 20/20 及 Core 导入/导出、Storage 集成 175/175 中；AAADAW 805/805、Core/App strict all-target Clippy、stable fmt 和 diff check 通过。该行为刻意复刻 REAPER 7.82 实测，不按 SMF 标准的 SMPTE wall-clock 解释；`MIDI-FILE-IMPORT-001` 与 P7 仍为 `in_progress`。

### 2026-10-18 将导入的 SysEx 按样本位置发送给 CLAP 插件

MIDI 播放计划现在保留预编译的 F0/F7 字节区间，并将 SysEx 作为带样本偏移的 CLAP MIDI 事件发送。进程内插件直接引用不可变计划字节；隔离插件使用固定共享内存区域传输，每个请求最多 16 KiB，音频回调不分配内存。Meta 事件仍不发送给乐器插件，物理 MIDI 输出的 SysEx 也尚未实现。

验证通过：MIDI 调度与引擎库/实时分配套件合计 126/126，隔离 helper 样本偏移 SysEx 集成测试 1/1，strict engine all-target Clippy (`-D warnings`)、stable fmt 通过。`MIDI-FILE-IMPORT-001`、UI-019 和 P7 继续为 `in_progress`；待处理包括超过单块上限的隔离 SysEx、物理 MIDI SysEx 输出、文件选择器及更多原厂导入边界行为。

追加的端到端回归从 960 PPQ 的 SMF 字节解码 F0@tick 0 与 F7@tick 240 continuation，经 REAPER 规则归并并导入 Project，再验证播放计划仅在 tick 240 所在的半开采样块发出一条完整 `F0 43 12 34 F7` 事件。这把文件解码、项目转换、持久事件和样本调度连成同一条测试路径。

### 2026-10-18 Android MIDI 文件选择修复与 Event List 终点属性窗

为修复 PR #282 上 Android ARM64 编译失败，SysEx/Meta 文件读写改走共享平台路径选择器与 Android SAF，不再直接引用桌面专用 `rfd`；MIDI 输入分派现在也显式处理 Program Change、Poly Aftertouch 和 Channel Pressure。另一条回归测试保证 MIDI Event Properties 模态打开时，SysEx/Meta picker 的取消结果仍可返回并清除忙状态。Linux strict Clippy、工作区格式检查、MIDI Event List app 全套 Nextest **806/806** 和 picker/modal 定向测试通过。提交 `3b372df` 与 `c1f7196` 的 GitHub CI 已通过 Linux、Windows、macOS、Android ARM64/x86_64 构建和 Android emulator foreground launch。

继续按原厂空 item fixture 补上 Event List 合成 `(end) All Notes Off` 行的 `Ctrl+F2` 入口。空 Item 唯一可见行在 AAADAW 进入 Event List 时仅有初始视觉选中、尚无显式内部游标；动作启用和打开路径已对齐该状态。现在弹出 Type、Position、Channel 均未设置的空白表单，Apply/OK 不创建工程事件，App 回归比较工程 snapshot 与 history，保持零修改。Xorg dummy :99 下用当前 Linux 桌面二进制真实输入 `Ctrl+F2` 并保存了 [属性窗截图](../verification/reaper-parity/fixtures/midi-event-list-empty-end-aaadaw-properties.png)，窗口前态截图、Build ID、二进制与图像哈希已记入 [fixture manifest](../verification/reaper-parity/fixtures/midi-event-list-ui.tsv) 和空 Item TSV。该空白表单的字段仍为 display-only，Apply 是 no-op；从空表单新建 MIDI 事件及 REAPER 对 Apply/OK 的原厂行为未实现/实测，因此 `MIDI-EVENT-LIST-001`、UI-019 与 P7 保持 `in_progress`。

### 2026-10-18 保留文本 Meta 中的无效 UTF-8 字节

已知文本类型的 Meta 若含无效 UTF-8，Event List 和 Event Properties 现在以十六进制显示原始字节，属性编辑器仍保留其 Meta 类型；只改 Position 不会重写 payload，明确编辑十六进制时也会按原字节提交并支持 Undo/Redo。新增 App 回归覆盖显示、Position-only 提交、字节编辑及历史往返。当前未对 REAPER 7.82 的无效 UTF-8 呈现做原厂实测，因此这里只修复 AAADAW 的字节可见性与保真，不宣称语义已与原厂对齐；`.sysex` picker、结构性 Meta 原厂字段与 Event Properties BBT 边界仍开放，`MIDI-EVENT-PROPERTIES-001` 继续为 `in_progress`。

### 2026-10-18 将 Quantize 扩展到三类已建模 MIDI 事件

Quantize Events 的 All events 和 Selected events 现在还会处理 Program Change、Poly Aftertouch、Channel Pressure；与 notes、CC、Pitch Bend 一样，移动基于当前编辑器网格和 Swing，所有类型组成一次历史编辑。Selected events 保留未选事件，仅当选中事件与未选事件冲突时优先保留选中事件；All events 同一目标位置仍沿用较晚原始 tick 优先。更新的 Core/App 回归覆盖三类事件的位置、碰撞、选中状态、Undo/Redo。REAPER 原厂目前只有 notes、CC、Pitch Bend 的已记录 Quantize 数据，因此这次扩展尚未声称这些新类型与原厂一致；SysEx/Meta、复杂重叠链与已记忆对话框设置仍开放，`MIDI-QUANTIZE-DIALOG-001`、UI-019 和 P7 保持 `in_progress`。

### 2026-10-18 MIDI Quantize 加入 SysEx/Meta 事件

All events 和 Selected events 的 Position 路径现在也移动不透明的 SysEx/Meta 行；原始 payload 字节、类型及源顺序保留。Selected events 只移动选中 opaque 行，并在 tick 排序后按事件身份恢复行选择；App 的 Undo/Redo 同样重新映射行索引。Core/App 回归覆盖无效 UTF-8 Meta payload、F0/F7 SysEx payload、未选行保留、跨行排序、只选 opaque 行时的量化、选择恢复及单次 Undo/Redo。Core MIDI quantize 套件 10/10、AAADAW 全套 808/808，workspace strict all-target Clippy (`-D warnings`)、fmt、diff check 和 112 行/15 列唯一 ID parity matrix 均通过；nightly Rust 1.101.0 Cranelift 桌面构建成功（Build ID `5f5396a0e4fa35067e65263cbc131b30161c1ee1`，SHA-256 `6656be75d66de3359e0623021372d4a2744b37d2070b1fcfc04c701d80f1d8ce`），只选 SysEx Quantize 的 Cranelift 定向测试 1/1 通过。没有 REAPER 7.82 对这些事件类型及同位置碰撞的 Quantize 原厂探测，故仅记录 AAADAW 自动化覆盖；本轮环境也没有 Xvfb/display，未新增桌面截图；`MIDI-QUANTIZE-DIALOG-001`、UI-019 和 P7 继续为 `in_progress`。

### 2026-10-18 补齐 MIDI Event List 的 SysEx/Meta 类型筛选记录

复核发现 AAADAW 已有 `Sysex/Meta` 过滤选项和 Event List 谓词：Track Name、Text、Marker、SysEx opaque 行可单独显示，行不带 channel 时仍通过 channel-only 筛选，Invert 会显示取反结果。回归 `event_list_shows_text_marker_and_sysex_item_events` 覆盖四行 opaque 事件、Sysex/Meta-only、channel-independent 与 inverted 结果。REAPER 原厂探针已记录菜单有 Sysex/Meta 项，但尚未逐行验证该筛选结果；当前 AAADAW 结果证据为自动化测试，没有新增实窗截图。因此只从“功能缺失”移到“实现待原厂结果对照”，`MIDI-CHANNEL-VIEW-001`、UI-018 和 P7 保持 `in_progress`。

### 2026-10-18 实窗核验 opaque MIDI Quantize 与历史往返

为补上此前缺少显示服务时的桌面证据，解包启动 Xvfb 并用 Mesa llvmpipe 在 Linux 当前 Cranelift 桌面二进制上打开可复现的 SysEx/Meta 工程。还补上 Piano Roll 中可见的 Piano Roll/Event List 模式按钮，并在新二进制中实点验证双向切换。Event List 中只选 Meta tick 475 和 F0 SysEx tick 595，保留 F7 tick 480 未选；在 Quantize Events 选择 Selected events → Position，以 1/32 网格应用后，Meta/F0 分别到 tick 480/600，F7 保持 480。事件因 tick 排序后，Meta 与 F0 的选中状态、Meta 原始字节及 F0/F7 SysEx payload 均保留。等待实窗重绘后，Ctrl+Z 恢复原 tick 和两行选择，Ctrl+Shift+Z 再应用。模式按钮、前后与 Undo 实窗截图、项目、生成器、二进制 Build ID/SHA-256 记录在 [Quantize Events fixture manifest](../verification/reaper-parity/fixtures/midi-quantize-dialog-ui.tsv)。先前 opaque quantize Core/App 套件 10/10、808/808 与 strict workspace Clippy 已通过；加入可见模式按钮后，本轮定向 App 回归 3/3、Cranelift 桌面构建、实窗交互、生成器运行、fmt、diff、parity matrix 和截图哈希核验均通过。REAPER 7.82 的 SysEx/Meta Quantize 原厂结果和 opaque 同位置碰撞仍未实测，故不宣称原厂一致，`MIDI-QUANTIZE-DIALOG-001`、UI-019 与 P7 继续为 `in_progress`。
### 2026-10-18 用 REAPER 7.82 实测 Meta/F0 SysEx Quantize 与同 tick 碰撞

以 REAPER 7.82/Linux、Default 7、Xvfb :299 和 1/32 straight grid 实测 `Quantize Events`。All events → Position → 100% 将 text Meta `475/484` 都移至 `480`，F0 SysEx `595/600` 都移至 `600`，Marker `715` 移至 `720`；同 tick 事件都保留，原始 payload、选择位和源顺序不变。Selected events 下，选中的 Meta `475` 到 `480`、选中的 F0 `595` 到 `600`；未选 Meta `484`、Marker `715` 和已在 `600` 的另一条 F0 不动，两个 F0 同 tick 共存。新增的 Core All-events 与 App Selected-events 回归锁定重复事件保留、payload/order 和选择历史；Core Quantize 11/11、AAADAW binary tests 787/787、定向 App 回归 1/1 均通过，目标 Core/App strict Clippy (`-D warnings`)、fmt 与 diff 检查通过。

可复现 ReaScript、两种 scope 的原厂 TSV、对话框截图、REAPER Build ID 与 SHA-256 均在 [Quantize fixture manifest](../verification/reaper-parity/fixtures/midi-quantize-dialog-ui.tsv)。独立 raw F7 在本次 SetAll/Sort 混合 take 读回中未保留，已有 standalone F7 原厂读回也没有独立事件，因此不把它算作已测的 quantize 行；有效 F7 continuation 仍按导入规则并入 F0。Program Change、Poly Aftertouch、Channel Pressure、多个 note overlap 链和 dialog settings 记忆仍在 P7 验收缺口内。

### 2026-10-18 实测 Program Change、Aftertouch 与 Channel Pressure 量化

在 Linux REAPER 7.82 / Default 7 / Xvfb :300 上，用 straight 1/32、Position、Strength 100% 分别完成 All events 与 Selected events。两种范围里，选中的 Program Change 与 Channel Pressure 从 475 移至 480，并占用同频道 tick 480 的唯一位置，原先未选的目标事件被移除；Poly Aftertouch 则保留同频道、同音符、同 tick 的不同压力值，Selected events 只移动所选值。All events 还将无关频道事件量化至 720，Selected events 保持它们原位。原厂读回、两个真实对话框截图、REAPER build/hash 和可重复脚本均录入 [fixture manifest](../verification/reaper-parity/fixtures/midi-quantize-dialog-ui.tsv)。

AAADAW 为 All events 添加选择优先碰撞参数，Core 与 App 回归覆盖 Program Change/Channel Pressure 取舍、Aftertouch 同位置保留，以及一次事务的 Undo/Redo；MIDI Event List 在量化及 Undo/Redo 后按事件身份恢复 CC、Pitch Bend、Program Change、Aftertouch、Channel Pressure 和 opaque 行选择。Aftertouch 模型现在接受 REAPER 允许的同位置多值并稳定保留事件顺序。Core Quantize **12/12**、Core Aftertouch **3/3**、AAADAW 主程序单测 **787/787**、Core/App strict Clippy (`-D warnings`) 均通过。`MIDI-QUANTIZE-DIALOG-001`、UI-019 与 P7 继续 `in_progress`：目前只验证了直拍 1/32 的 Position；其他网格/Swing/强度/操作、复杂重叠链和已记忆的对话框设置仍待测。


### 2026-10-18 Selected multi-note overlap-chain Quantize 修正

继续探测 REAPER 7.82 / Default 7 的 Selected notes、Position and note length、1/32、100%、Fix overlaps 关闭：同时选择第二和第四个同音音符后，选中的 raw NoteOff 分别移动至 360 与 720 tick，未选音符的 NoteOff 仍在 300、480、650、760、880。`MIDI_GetNote` 将这些同音重叠事件重新配对到不同的表格行，因此此前把它当成 note identity，错误地缩短选中音符并改写邻居时值；现在 Core 让选中音符自己的 NoteOff 随量化长度移动，未选音符保持原端点。单选五音探针、双选七音探针的原始事件、`MIDI_GetNote` 读回、原厂 dialog 截图和脚本都收录在 [Quantize fixture manifest](../verification/reaper-parity/fixtures/midi-quantize-dialog-ui.tsv)。

新增 Core 多选链回归及 Undo/Redo，修正先前单选测试的真实事件端点预期；App dialog 回归现在同时选两音、跨越多个未选同音邻居，并验证未选音符端点不变。Core MIDI Quantize **13/13**、定向 App Quantize **12/12**、完整 AAADAW 主程序测试 **787/787**、Core/App all-target Clippy (`-D warnings`)、`cargo fmt --check` 与 `git diff --check` 均通过。当前源码桌面在 Xvfb :300 实际打开工程、选择 note 2、执行对话框量化，并在 Event List 中确认 `1.1.18 → 1.1.25`、长度 `0.12`，邻音保持 `1.1.34 / 0.15`；实窗 Ctrl+Z/Redo 往返与二进制身份、截图哈希也已归档。本次 GUI 使用 Stable Rust build；该工作目录没有可复用的 Cranelift 依赖输出。`MIDI-QUANTIZE-DIALOG-001`、UI-019 与 P7 仍为 `in_progress`：其他混合选择重叠排列、Fix overlaps 全部交集和对话框设置记忆尚未验证。

### 2026-10-18 MIDI Quantize settings memory

REAPER 7.82 / Default 7 was tested in a fresh Linux profile: changing all five Quantize Events controls then using Cancel preserved them on reopen; a second value set preserved after OK, clean application close, and restart. The [native fixture](../verification/reaper-parity/fixtures/midi-quantize-settings-memory-native.tsv) records the two dialog captures, restart screenshot, probe, and profile readback. AAADAW now remembers scope, operation, strength, Bypass, and Fix overlaps across Cancel/OK and application restarts using an atomic versioned user-config file; default values match REAPER. App regressions cover reopen after Cancel and OK, preference-file round-trip and invalid values. Targeted Quantize dialog tests passed 12/12 and preference serialization/validation tests passed 2/2. An initial headless run exposed the CLAP helper test’s X11 requirement; the full workspace Nextest rerun under Xvfb passed 1533/1533 with 0 skipped; strict Core/App/desktop Clippy with `-D warnings`, formatting, matrix and fixture hash checks passed. A current Stable Linux desktop run changed all five controls, clicked Cancel, confirmed the saved JSON, reopened the dialog and then restarted the process under the same XDG profile; the [AAADAW fixture](../verification/reaper-parity/fixtures/midi-quantize-settings-memory-aaadaw.tsv) includes before/after/restart screenshots, build identity and the persisted JSON. MIDI Quantize dialog matrix remains `in_progress` because event behavior and other overlap cases are still under active investigation.

### 2026-10-18 Arrange MIDI 内嵌编辑器基础操作与实窗核验

REAPER 7.82 / Default 7 在 Linux 新配置中打开现有 MIDI item，实按 `E` 打开 Arrange 内嵌编辑器，再按 `E` 返回普通 Arrange 预览；原厂前后截图与使用的 `midi-note-quantize-before.rpp` 已登记。AAADAW 为选中的 MIDI item 增加相同的 `E` 开关、独立关闭 Action 和 item 菜单入口；内嵌音符区域可按当前网格绘制、移动和拖动右边缘改长度，Shift 绕过 Snap，Escape 取消未提交手势。每次提交走既有 MIDI 编辑事务，可由 Undo/Redo 恢复。

在最新 Linux 桌面程序中从空工程新增 track/item，实按 `E` 后绘制 B4（tick 960、960 ticks），拖至 C#5（tick 1920），Ctrl+Z 恢复、Ctrl+Shift+Z 重做，再延长音符至 1920 ticks；截图显示稳定音高网格和同步 Inspector。关闭 `E` 后音符回到 Arrange 预览。音高、时间移动、绘制和右边缘长度处理也由 Timeline 回归覆盖；AAADAW 桌面测试 792/792、严格 all-target Clippy（`-D warnings`）、fmt 与 diff check 通过。配对截图、输入步骤、Build ID 与文件哈希列在 [inline editor fixture manifest](../verification/reaper-parity/fixtures/midi-inline-editor.tsv)。

原厂本轮只确认了 `E` 的开关，不据 AAADAW 的行为推定原厂绘制、移动、调整音长或 Undo 细节一致。AAADAW 的 item 菜单入口仍需实窗操作核对；主题/布局、音高缩放与音域、loop source 边缘、多 item 与其他鼠标映射也尚未逐项对照。`MIDI-INLINE-EDITOR-001`、UI-020 与 P7 保持 `in_progress`。

### 2026-10-18 空白 MIDI item 网格与插入手势原厂对照

在同一 REAPER 7.82 / Default 7 隔离配置内新建空 MIDI item 并按 `E`，确认即使没有音符也会显示音高网格。空白格单击后，REAPER 编辑器窗口前后 PNG 的 SHA-256 完全相同；双击会插入当前 1/4 网格长度音符，左键拖动也会按拖动范围和网格创建音符。原厂空白打开、单击、双击、拖动截图，以及单击前后的相同 1024×768 编辑器图像均保存在 [inline editor fixture manifest](../verification/reaper-parity/fixtures/midi-inline-editor.tsv)。

AAADAW 空白已打开 MIDI item 现在绘制默认稳定音高网格；空白格单击不改工程，双击插入默认网格长度音符，拖动按网格跨度插入。Cranelift 桌面实窗捕获打开、单击、双击和拖动四种状态；最新 binary Build ID 与 SHA-256、10 幅新增参照/产品截图的哈希均登记在 manifest。AAADAW 主程序测试 **794/794** 通过，含空白 pitch grid、单击无编辑、双击插入和拖动时长回归；Cranelift desktop build、stable all-target Clippy（`-D warnings`）、fmt 与 diff check 通过。

本次只对齐了空白 item 的网格可见性和在默认 1/4 网格上的插入手势，不据此推定原厂音高映射、移动/调整长度、滚轮缩放、音域策略、多 item、loop source 边界或菜单入口等价。`MIDI-INLINE-EDITOR-001`、UI-020 与 P7 继续 `in_progress`。

### 2026-10-18 Arrange inline MIDI 已选音符组移动

在 Linux REAPER 7.82 / Default 7 中以 `midi-note-quantize-before.rpp` 实测 1/32（120 tick）网格：已选 C4/D4/F4 原位于 100/350/900 tick，未选 E4 位于 550；拖动 C4 至 840 后，REAPER 将其余已选音符同步移至 1090/1640，保留 +250/+550 相对偏移、音高和 120 tick 音长，未选 E4 不动。原厂前后窗口截图及保存后的 RPP 读回已归档。

AAADAW inline MIDI 增加每个 item 独立的音符选择、Ctrl 点击增删、选中音符蓝色高亮和整组选中音符的共同时间/音高位移；非循环 MIDI source 上整组时间与音高增量共同限界，编辑通过一次 `EditMidiNotes` 列表提交。Cranelift 桌面程序实窗按相同 tick/网格操作，保存后读回为 840/1090/1640，未选 E4 仍为 550；选区和移动前后截图、工程文件及 SHA-256 在 [inline editor fixture manifest](../verification/reaper-parity/fixtures/midi-inline-editor.tsv)。新增控制点击选择、原厂 tick 值组移动和 `inline_midi_group_edit_is_one_undoable_transaction` 应用层 Undo/Redo 回归后，AAADAW 桌面测试 **797/797** 通过；Stable all-target Clippy (`-D warnings`)、fmt 与 diff 检查通过。

`MIDI-INLINE-EDITOR-001`、UI-020 与 P7 继续 `in_progress`：REAPER 本轮只验证了已选音符的水平组移动，逐音音高映射、resize 手柄、loop-source 边界、多 item 选择与鼠标缩放仍未对照；不得据该组用例判定其他 MIDI 编辑手势已对齐。

### 2026-10-18 Arrange inline MIDI Ctrl-drag 复制所选音符组

REAPER 7.82 Linux / Default 7 已实测 1/32（120 tick）网格上的 Ctrl 拖拽：从 C4 的 840 tick 位置右移 120 tick 后，原 C4/D4/F4 保持在 840/1090/1640 并取消选中；新副本选中在 960/1210/1760，未选 E4 保持 550，音高与 120 tick 音长不变。前后截图与两份保存 RPP 的 raw MIDI event 读回均已登记。

AAADAW inline MIDI 现在将 Control/Command 拖动映射为副本事务：拖动选中音符组时保留原组并按共同时间/音高增量创建新音符，释放时选中新副本；无实际拖动时才提交修饰键点击的选择切换，避免 Ctrl 拖动起始先把锚点从组选择中移除。通过已有 `CopyDragMidiNotes`/`AddMidiNotes` 一次提交并同步编排视图与钢琴卷帘选择。最新 Cranelift 桌面程序在 Xvfb :314 对照同一工程实际拖动、保存并读回精确 7 音状态；实窗 Ctrl+Z 恢复 4 个源音符及源组选区、Ctrl+Shift+Z 重做 7 个音符及副本组选区。应用历史同时保存 inline 与已打开钢琴卷帘的前后选区；新增手势/事务回归后桌面测试 **799/799** 通过；Stable all-target Clippy（`-D warnings`）、fmt 与 diff check 通过，Build ID 与 SHA-256、前后/Undo/Redo 截图和保存工程已列入 [inline editor fixture manifest](../verification/reaper-parity/fixtures/midi-inline-editor.tsv)。

`MIDI-INLINE-EDITOR-001`、UI-020 与 P7 仍为 `in_progress`：本次确认了原厂 Ctrl-drag 复制与 Ctrl-click 增删选择映射；音高操作、resize 边界、zoom/range、loop source、多 item 等仍需各自实测，不能据此判定内嵌 MIDI 编辑器整体对齐。

### 2026-10-18 REAPER MIDI Editor Ctrl-click 选择增删核验

在 REAPER 7.82 / Linux / Default 7 的 MIDI Editor 中，从 Ctrl 拖拽后的状态继续操作：Ctrl 点击已选副本 C4（960 tick）只取消该音符选择，D4/F4 副本仍选中；再 Ctrl 点击未选源 C4（840 tick）只添加源音符，C4 副本继续未选。两个 MIDI Editor 截图和保存后的 RPP 原始事件大小写选择标记已加入 [inline editor fixture manifest](../verification/reaper-parity/fixtures/midi-inline-editor.tsv)，读回确认事件 tick、音高、音长均未改变。

原厂后续 Ctrl+Z 将刚加入的音符从选区移除但不改 MIDI 内容，Ctrl+Shift+Z 重做后恢复该选区；因此 Control-click 选择切换本身属于 Undo/Redo 顺序。

AAADAW 的 Control-click 处理通过同一释放态事件流回归覆盖：Ctrl 点击未选 note 延后到 MouseButtonReleased 才加入；再次点击已选 note 延后到 release 并只移除该 note，其余选择保留。现将选择状态作为应用层历史处理，Undo/Redo 只恢复 Timeline 与打开的 Piano Roll 选区，不建立 MIDI 内容快照；同一历史栈保持 Ctrl-drag 副本事务与之后 Ctrl-click 选择动作的顺序。新增 App 回归覆盖空选区增选/取消和“复制组→改变选区→两次 Undo/Redo”的交错序列，项目快照全程不变。Cranelift 主程序测试 **800/800**、稳定工具链 Nextest **800/800**、workspace strict all-target Clippy（`-D warnings`）、fmt 与 diff 检查通过。Cranelift 构建仍给出既有 iced/wgpu `TimelinePipeline: MaybeSync` recursion-depth warning。

最新 Cranelift 桌面二进制 SHA-256 `84875ca1a7d2496288e05863a8ad5128a8aaa6880c9614d98fa057d4adbbf558` 在隔离 XDG 配置和 Xvfb :316 真实打开七音符工程；实按 E 打开 inline editor、Ctrl-click 加入 C4 副本、Ctrl+Z 清除蓝色选区、Ctrl+Shift+Z 重现蓝色选区。前态、选择态、Undo、Redo 四张 1280×800 实窗截图及哈希均在 [inline editor fixture manifest](../verification/reaper-parity/fixtures/midi-inline-editor.tsv)。`MIDI-INLINE-EDITOR-001` 继续 `in_progress`；原厂 Ctrl-click 与选择 Undo/Redo 现已对照，item context menu、音高拖动、resize 边界、zoom/range、loop source、多 item 等仍待独立验收。

### 2026-10-18 MIDI Editor Piano Roll 选音符撤销

提交 `faf0af3` 将 MIDI Editor 的 `SelectMidiNotes` 接入应用层选择历史，并在 MIDI Editor 开启时同步 Arrange inline 选区。两次连续选择后，Undo/Redo 在两处视图都逐步恢复选区；回归同时确认工程内容快照不变。Cranelift 桌面测试 **801/801**、Stable Nextest **801/801**、workspace strict all-target Clippy（`-D warnings`）、fmt 与 diff 检查通过。测试记录在 [inline editor fixture manifest](../verification/reaper-parity/fixtures/midi-inline-editor.tsv)。

本增量没有独立 Piano Roll 实窗截图；先前 Xvfb 实窗证据只验证 Arrange inline editor。`MIDI-INLINE-EDITOR-001`、UI-018/UI-020 与 P7 保持 `in_progress`；仍需实测 REAPER Piano Roll 点击/修饰键选择的 Undo 历史，以及 Event List 选择、工具/音高映射、resize、zoom/range、loop-source 与多 item 行为。

### 2026-10-18 MIDI Editor 多类型事件选择 Undo/Redo

提交 `f696fd4` 将 MIDI Editor 选择历史扩展为跨类型快照，覆盖 Note、CC、Pitch Bend、Program Change、Aftertouch、Channel Pressure、opaque event 的选区，以及 Event List 当前行和锚点；Piano Roll、Event List 行、CC/Pitch Bend lane 点击、Event List Arrow 与 Home 选择现共用历史，并同步 note selection 到 Arrange inline 选区。此状态只存在于应用层，不改变工程内容或存储 schema。Undo/Redo 回归验证混合 Note/CC 选区、两视图同步、键盘行选择/范围行为及工程快照不变。

最终 Cranelift 桌面测试 **802/802**、稳定 Nextest **802/802**、workspace strict all-target Clippy（`-D warnings`）、fmt 与 diff 检查通过。当前生产二进制 Build ID `ef5af2d5bfff238c851f69d665348bfadc4533b2`、SHA-256 `6a969b90decbd64766661d8f7cf8564a9c4bcead4aed6a783c8adea90b93cf9a` 在 Xvfb :319 实际打开 7 音符工程；实窗选择 C4、Ctrl-click D4、Ctrl+Z 和 Ctrl+Shift+Z 的四张截图与 hashes 在 [Event List selection fixture](../verification/reaper-parity/fixtures/midi-event-list-selection-history-aaadaw.tsv)。

原厂 REAPER 已确认 MIDI Editor Piano Roll note Ctrl-click 选择会进入 Undo/Redo 顺序；Event List row-click/Arrow/Home 选择的专项 Undo 历史仍须独立原厂 probe，AAADAW 截图不代替该验收。`MIDI-EVENT-LIST-001`、`MIDI-INLINE-EDITOR-001`、UI-018/UI-019/UI-020 与 P7 继续 `in_progress`；下一步先实测 REAPER Event List 的鼠标/键盘选择与 Undo/Redo，再处理 REAPER Data 列、filter 规则与未建模 Meta 行等开放项。

### 2026-10-18 Event List REAPER Data 列与窄窗口横向滚动

提交 `ab5b886` 将 REAPER 7.82 原厂列序 `Index / Position / Length / Channel / Type / Parameter / Value / Off Value / REAPER Data` 接入 AAADAW Event List。表格在竖向滚动区域内再嵌一层横向滚动；实窗检查发现直接使用 Iced 双向滚动会令表格完全空白，当前布局已改为窄窗口下可将末列滚入视区，并保留完整行渲染。Linux Xvfb :320 当前 Cranelift 桌面构建在 1380×780 MIDI Editor 中显示完整九列；缩至 800×620 并横向滚动后能看到 REAPER Data。Build ID `5e34df6ac45ef5ba3d3f8dca92dc47f9d1353244`，SHA-256 `ca37c1f764a31ba394decfbca4895f446a3c5b87ab4bd31f726251033d6c4798`；两张真实窗口截图、七音符项目来源、哈希和验证过程记录在 [REAPER Data fixture manifest](../verification/reaper-parity/fixtures/midi-event-list-reaper-data-aaadaw.tsv)，总表也已登记。

REAPER 原厂 Note、Program Change、Poly Aftertouch 和 Channel Pressure 截图中的 REAPER Data 单元格为空；AAADAW 这些行现留空并与已测结果相符。Meta/SysEx 的 REAPER Data 含义与显示仍未实测，故没有填入猜测值。最终 Cranelift 测试 **803/803**、稳定 Nextest **803/803**、workspace strict all-target Clippy（`-D warnings`）、fmt 和 diff check 通过。`MIDI-EVENT-LIST-001`、UI-019、P7 继续 `in_progress`；opaque 数据、filter controls、混合类型属性、排序、列宽与菜单等仍待逐项验收。

### 2026-10-18 原厂核验 Event List 行选择进入 Undo/Redo

在 Linux REAPER 7.82 / Default 7 的 Event List 中，以 ReaScript 创建四音符 item（前 3 行已选、第 4 行 C5@tick 1440 未选），Ctrl-click 第 4 行后保存，原始 MIDI take 的四个 Note On/Off selection bits 全部变为 selected。立即 Ctrl+Z 再读回，第 4 行的 Note On/Off bits 恢复为 unselected；Ctrl+Shift+Z 重做后，两者重新变为 selected。前 3 行选中状态、所有事件 tick/length/channel/pitch/velocity 均不变；RPP diff 除保存时间戳外只改第 4 行的选择位。原厂 Linux 二进制 Build ID `fb9ade2f211bc91a3ee6193630ef17bcf5f6528b`、SHA-256 `9976f5608068b7a58518969001b00b527fb0de6ed7459e0e5dda6f29878ee254`，Default 7 theme、生成脚本、Undo/Redo 实窗截图与两份 RPP 读回均在 [Event List selection history native fixture](../verification/reaper-parity/fixtures/midi-event-list-selection-history-native.tsv)。

因此 AAADAW 已有的 Event List Ctrl/Cmd-click、Arrow、Shift-range 与 Home 选择 Undo/Redo 现获得 REAPER Event List 鼠标选择的原厂历史顺序对照；此前 MIDI Editor Piano Roll 选择历史的原厂对照也已记录。当前 Cranelift/Nextest **803/803**、strict workspace Clippy、fmt/diff 均已通过。更广的 Event List 过滤字段/动作、Meta/SysEx REAPER Data、混合类型属性、排序/列宽和上下文菜单仍未完成；`MIDI-EVENT-LIST-001`、UI-019 与 P7 维持 `in_progress`。

### 2026-10-18 Event List Filter Set/Add/Remove 选择操作

AAADAW 的 Event List Filter Events 对话框现有原厂同名的 `Set selection`、`Add to selection`、`Remove from selection` 操作。操作按事件类型、通道 mask 与反转条件匹配所有 modeled MIDI 事件及 Meta/SysEx；Show-only 只控制可见行，合成 `(end)` 行不进入选区。Set 替换不同类型的旧选区，Add 合并，Remove 扣除；选择进入既有 Undo/Redo 历史并同步 Arrange inline note selection，不改工程数据。P7 原子验收表已补入这项明确要求。

Linux REAPER 7.82 / Default 7 原厂探针使用 Note + channel 5、Show-only 开启：Set 单独选中 channel 5，Add 从前 3 个选中扩展为 4 个，Remove 从 4 个选中回到前 3 个。原厂截图及三份 RPP 读回，与 Xvfb 当前 AAADAW 实窗的 Set / Add / Remove 截图和 SHA-256 均已登记在 [筛选选择验证页](../verification/reaper-parity/midi-event-filter-selection.md)及两个 fixture manifests。后续关闭 Show-only 的原厂探针确认 Set 仍只选中 channel 5；从四条全选开始 Remove 只清除 channel 5；从 channel 2–4 选中开始 Add 又选中 channel 5。三组 RPP 与截图均已加入原厂 manifest，其他筛选组合仍待探测。

App 回归覆盖 Notes、CC、Pitch Bend、Program Change、Poly Aftertouch、Channel Pressure、Meta/SysEx、反转、Undo/Redo、Arrange 选区同步与工程快照不变。Cranelift 全 workspace 在 Xvfb :324 下 **1,548 项测试、84 个测试目标全部通过**；Stable workspace strict all-target Clippy (`-D warnings`)、fmt 与 diff 检查通过；两个新 manifest 的图像/RPP 哈希逐项校验通过。截图所用 AAADAW 桌面二进制 Build ID `d3fc747138cb692ae7d198cbe33f84720b29224c`、SHA-256 `62bb3d82813fd9cabe5e4c83a185e05b10501cd8bab3134f6b514b1496b2fe5e`。Cranelift 保留既有 `TimelinePipeline: MaybeSync` future-incompatibility warning。

`MIDI-EVENT-LIST-001`、UI-019、P7 继续 `in_progress`。下一项继续用 REAPER 实测其他筛选条件，再扩展 Parameter/Value/Position、Solo 与 Set filter from selection 控件；这些未完成项不因本次选择操作完成而关闭。


### 2026-10-18 Event List Filter Clear filter 与隐藏选区保留

AAADAW 已实现 `Clear filter`：关闭 Show-only / Invert，恢复全部通道与事件类型，编辑器通道设为 All channels，绘制通道重置为 Channel 1；保留筛选对话框与当前事件选区，不更改工程数据或 Undo 历史。原厂 REAPER 7.82 / Default 7 实测确认这些重置值，Invert 会清除而 Solo 会保留；四条 Note 的工程事件和选中位不变。原厂前后截图与 RPP 读回见 [Clear filter 原厂记录](../verification/reaper-parity/midi-event-filter-clear.md)。

Xvfb 当前 AAADAW 实窗前后截图复现 Note + channel 5 + Show-only + draw channel 5，再 Clear；四条 Note 全程保持选中，清除后全部重现且合成 end 行可见。该对照发现筛选隐藏行时不能删除其选区；已调整过滤状态更新并加入隐藏选区回归。截图与当前二进制 Build ID `1560378bb71947a856c19deba9d80477b99f2633`、SHA-256 `11baaa496c7e10524f33086291378a124e4acd1e615d6ec1f907500d09029a5f` 记录在 [AAADAW Clear filter manifest](../verification/reaper-parity/fixtures/midi-event-filter-clear-aaadaw.tsv)。Cranelift AAADAW 套件 **826/826**、Stable workspace strict all-target Clippy (`-D warnings`)、fmt 与 diff check 通过。

Parameter、Value、Position、Repeat、Length、Solo 控件及 Set filter from selection 仍未实现；前五项 Clear 的 reset 语义也尚未逐项原厂验证。`MIDI-EVENT-LIST-001`、UI-019、P7 继续 `in_progress`。


### 2026-10-18 MIDI Event Filter 的 CC Parameter / Value

AAADAW 的 Filter Events 现能筛选 Control Change 参数与闭区间 Value。REAPER 7.82 / Default 7 原厂实测以四个 CC 建立边界案例：CC7 的 64 与 100、CC10 的 64、CC7 的 127；选择 Control Change、`07 Volume MSB`、Value `64–100` 并开启 Show-only 后，只剩 CC7 的 64 和 100。REAPER MIDI readback 与保存的 RPP `EVTFILTER 0 176 -1 64 100 0 1 1 0 -1 -1 -1 -1 0 -1 1 7 7`、原厂前后截图和可重跑 ReaScript 均在 [原厂 fixture](../verification/reaper-parity/fixtures/midi-event-filter-cc-native.tsv)。原厂 Linux binary Build ID `fb9ade2f211bc91a3ee6193630ef17bcf5f6528b`、SHA-256 `9976f5608068b7a58518969001b00b527fb0de6ed7459e0e5dda6f29878ee254`。

AAADAW 以同样四个 CC 和三条 Note 进行 Xvfb :510 实窗复核：Control Change 下启用 `07 Volume MSB`、Value `64–100` 后，筛选对话框显示正确范围；Show-only 关闭时八行全在，开启后仅 CC7 的两个边界行可见。应用消息、CC lane 可见性/命中测试、Event List 渲染与 Set/Add/Remove 选择共用该条件，Clear filter 重置 Control/Value；两项回归同时检查边界匹配、未匹配控制器/值、终端行隐藏与工程快照不变。可重建 AAADAW 工程的 example、项目、三个实窗截图、当前 Cranelift binary SHA-256 `5973af52b9f257a00e0fe8880a27132085aa8921dcbbda5a75647b6157fc8c29` / Build ID `bc81aca8cf81dcf9d3bdab6f18cc2b185e8233e6` 与逐文件哈希在 [AAADAW fixture](../verification/reaper-parity/fixtures/midi-event-filter-cc-aaadaw.tsv)。功能范围与原厂证据见 [Parameter / Value 验证记录](../verification/reaper-parity/midi-event-filter-parameter-value.md)，Event List 与 channel control 当前说明和验收矩阵也已更新。

Stable 与 Cranelift AAADAW 全套测试各 **833/833**、workspace all-target strict Clippy（`-D warnings`）、Cranelift 桌面构建、fmt、diff 和两份 fixture hash 检查均通过。`MIDI-EVENT-LIST-001`、`MIDI-CHANNEL-VIEW-001`、UI-019 与 P7 仍为 `in_progress`。尚未关闭的工作包括 Control/Value 的原厂 Clear 与 Invert 语义、CC0/32、其他 CC 参数、从 CC 选区推导筛选条件、Repeat、非 Note Position/Length 与 Solo；下一步继续先测量原厂筛选面板的 Repeat 和非 Note Position 规则，再逐个实现与对照。

### 2026-10-19 MIDI Event Filter Position / Repeat（Note 与 CC）

Linux REAPER 7.82 / Default 7 的四小节原厂工程包含 960 PPQ、4/4、每 120 ticks 一个 CC7 的 128 个事件。Position in measure 设为 `1/8`–`1/8` 时，Repeat every `measure`、`1/2`、`1/4` 分别显示 4、8、16 个 CC，命中 tick 与 MIDI readback、RPP `EVTFILTER` 状态一致。边界探测确认 Position `3/4` 在 Repeat `measure` 时显示 4 个，在 Repeat `1/4` 时显示 0 个，筛选位置不会回绕到短周期内。可重跑 ReaScript、工程读回及原厂截图见 [native fixture manifest](../verification/reaper-parity/fixtures/midi-event-filter-position-repeat-native.tsv)。

AAADAW 现在以单一 Position range 与 Repeat period 过滤 Note 和 CC，并将相同条件用于 Event List、CC lane、Set/Add/Remove selection。Repeat choices 覆盖 `1/8T`、`1/8`、`1/4T`、`1/4`、`1/2T`、`1/2`、`measure`；Position 当前使用 0–1 的归一化数值输入。App 回归对 Note 和 CC 都核验 Measure/1/2/1/4 的事件数与位置，并补入 Position `3/4` 下 Measure 命中而 Repeat `1/4` 无匹配的原厂边界。实际打开 AAADAW Cranelift 桌面窗口，选 CC7、Position `0.125`–`0.125`、Repeat `1/4` 后，Event List 报告 16 条，实窗截图和二进制身份在 [AAADAW fixture manifest](../verification/reaper-parity/fixtures/midi-event-filter-position-repeat-aaadaw.tsv)。

Cranelift `aaadaw --bin` 单测 **813/813** 通过；Stable workspace all-target Clippy (`-D warnings`)、fmt 与 `git diff --check` 通过。REAPER 与 AAADAW 截图是实机行为证据，不代表像素级一致验收。剩余工作包括其他 MIDI 事件类型的 Position、非 4/4 meter、CC selection 推导 Filter、Position 的原厂 musical-value picker，以及 Solo 和其他尚未测量的过滤组合；`MIDI-EVENT-LIST-001`、UI-019 与 P7 继续 `in_progress`。


### 2026-10-19 Event List Filter 从同参数 CC 选区生成筛选条件

AAADAW 的 `Set filter from selection` 现支持同一 Control Change 参数的多事件选区。它从所选 CC 推导 MIDI 通道 mask、Control Change 类型、控制器编号、闭区间 Value 和 measure Position；多通道时将编辑器通道设为 Multichannel，Repeat every 设为 measure，绘制通道回到 Channel 1。混合 CC 参数、混合事件类型及其他 CC 以外的事件类型仍保持未实现。

Linux REAPER 7.82 / Default 7 原厂探针选中 channel 2 的 CC7 value 64 @ tick 480 与 channel 3 的 CC7 value 100 @ tick 960。`EVTFILTER` 读回为 `EVTFILTER 6 176 -1 64 100 1 1 1 0 -1 -1 0.125 0.25026042 1 -1 1 7 7`；保存前后 MIDI source 与 CC 选中位未变化。原厂脚本、Event List/Filter Events 截图和 RPP 读回在 [native fixture manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-cc-native.tsv)。

AAADAW Cranelift 当前实窗截图复核了推导的 channels 2–3、Control `07 Volume MSB`、Value 64–100、Position 0.125–0.25026042、Repeat `measure`、Multichannel 和 draw channel 1；对话框关闭后两条 CC 仍选中，开启 Show-only 后可见行正好是 value 64 和 100。项目生成器、来源工程、三张实窗截图、二进制 SHA-256 `c242de990decaf6f85dbc504c481e55b9b006af986842b3c9f9b6686a0b390f9` / Build ID `41a64de0ae0b1a91746de21039e797c4fd7a4515` 与逐项哈希见 [AAADAW fixture manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-cc-aaadaw.tsv)。App 回归检查推导条件、选区、工程快照与 Undo/Redo 历史均符合预期。AAADAW 测试套件 **814/814**、Stable workspace all-target strict Clippy（`-D warnings`）、fmt、Cranelift 桌面构建及 `git diff --check` 通过；Cranelift 保留已知 `TimelinePipeline: MaybeSync` future-incompatibility warning。截图验证行为和字段，不构成像素级一致证明。

`MIDI-EVENT-LIST-001`、UI-019、P7 仍为 `in_progress`。下一步继续验证和实现 Event List 其他筛选类型、Repeat 的非 4/4 行为与其余未覆盖交互。

### 2026-10-19 混合 CC 参数筛选与 Event List Filter 状态

Linux REAPER 7.82 / Default 7 原厂探针选中 channel 2 的 CC7/value64@480 与 channel 3 的 CC10/value100@960。`Set filter from selection` 保留通道 2–3 和 CC 类型，Control 变为禁用的 `<all>`，Value 得到 64–100，Position 得到 0.125–0.25026042，Repeat 为 measure；Show-only、Invert、Solo 保持关闭。执行 Set 后由操作者显式启用 Event List 漏斗，列表显示四条符合 channel、type、Value、Position 的 CC 行，其中包括未选中的 CC1 和 CC10；两条选中事件仍保持选中。RPP `EVTFILTER` 与 before/after MIDI event lines 证实 MIDI 源事件和选中位未变；脚本、读回、RPP 和原厂截图见 [mixed CC native manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-mixed-cc-native.tsv)。

AAADAW 对同一参数 CC 生成 Control 筛选条件，对混合参数将 Control 设为 `<all>`；两者都启用 Value/Position，但 Set 操作将 Event List Filter 关闭，用户显式启用后才应用筛选。原厂实窗组合测试还确认 Show-only 勾选会启用漏斗；关闭漏斗会清除 Show-only 并恢复全部七行。AAADAW 沿用该状态关系，独立 Filter 开启时 Show-only 可保持关闭。当前构建的混合 CC 实窗截图、fixture 工程和身份信息见 [AAADAW manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-mixed-cc-aaadaw.tsv)。回归验证筛选字段、四条可见 CC、独立 Filter 与 Show-only 状态、Clear、选区、工程快照和 Undo/Redo 历史。Cranelift `aaadaw --bin` **816/816**、Stable workspace all-target strict Clippy（`-D warnings`）、Cranelift 桌面构建、fmt 和 diff 检查通过；保留已知 `TimelinePipeline: MaybeSync` future-incompatibility warning。截图验证行为，不是像素级一致证明。

混合事件类型以及 Program Change、Aftertouch、Channel Pressure、Meta、SysEx 选区仍未测量；`MIDI-EVENT-LIST-001`、UI-019、P7 保持 `in_progress`。

### 2026-10-19 Pitch Bend filter from selection 与 Event List 值显示

Linux REAPER 7.82 / Default 7 选中 channel 2 的 raw Pitch Bend 4096 @ tick 480 和 channel 3 的 12288 @ tick 960，另有未选中的 10000 @ tick 600。`Set filter from selection` 得到 Pitch 类型、通道 2–3、有符号范围 −4096…+4096、Position 0.125–0.25026042；Repeat every 对 Pitch 禁用。漏斗关闭时六行仍显示；显式开启后只显示三条匹配的 Pitch Bend（含未选中的 +1808）。选区和 MIDI 源数据不变。原厂对话框、Event List 前/后截图、RPP、MIDI readback 与 ReaScript 在 [Pitch Bend native manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-pitch-native.tsv)。

AAADAW 的 `Set filter from selection` 已支持同类型 Pitch Bend 选区，推导通道、有符号弯音值和 Position，并在操作结束时关闭 Event List 漏斗；手动启用后按这些条件筛选 Event List。Cranelift 构建的实际 MIDI Event List 与过滤对话框截图展示 6 行未筛选状态和 3 行筛选状态；Pitch Bend 位于 Type 列、Parameter 留空，值以 8192 为中心显示（例如 −4096、+1808、+4096），与 REAPER 一致。截图由本次重建的桌面程序在独立 Xorg :545 窗口中现场采集。项目生成器、`.aaadaw` fixture、截图、二进制身份和哈希见 [AAADAW Pitch Bend manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-pitch-aaadaw.tsv)。

Cranelift AAADAW 主程序完整测试 **818/818**、Focused Set-from-selection 测试 **5/5**、Pitch Bend centered-value 格式测试 **1/1**、workspace Nextest **1,562/1,562（78 个测试目标，0 skipped）**，以及 `cargo +nightly -Zcodegen-backend clippy -p aaadaw --all-targets -- -D warnings -A future-incompatible` 均通过；`cargo fmt --all -- --check` 与 `git diff --check` 通过。Nextest 在绝对 `CARGO_TARGET_DIR` 和 dummy Xorg 下运行，使 CLAP fixture 与窗口测试可复现。已知 `TimelinePipeline: MaybeSync` future-incompatibility warning 仍由 Cranelift 报告并单独允许。`MIDI-EVENT-LIST-001`、UI-019 和 P7 保持 `in_progress`：混合事件选区、Aftertouch/Channel Pressure/Meta/SysEx 派生规则、更多筛选组合和完整原厂交互仍待实现与验证。

### 2026-10-19 Program Change Filter from selection

Linux REAPER 7.82 / Default 7 选中 channel 2 的 program 8 @ tick 480 与 channel 3 的 program 64 @ tick 960，并以未选中的 program 32 @ tick 600 和 CC7 作为控制。`Set filter from selection` 推导 Program Change 类型、通道 2–3 与 Position 0.125–0.25026042；Program 数值范围保持未启用且为空，Repeat 为 measure，绘制通道为 2。Set 后 funnel 关闭、六行仍可见；显式开启 Show-only 后仅三条 PC 可见，未选中的 program 32 也通过。原厂 MIDI readback、EVTFILTER、before/after RPP 与四张实窗截图见 [native manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-program-native.tsv)。

AAADAW 已实现相同的 Set-from-selection 推导、可选闭区间 Program 值条件以及 Event List 的 Set/Add/Remove selection 匹配。Cranelift 桌面程序在 dummy Xorg :547 的真实窗口截图验证了筛选对话框、Set 后漏斗关闭与六行未筛选列表、手动启用后 8/32/64 三行列表；两条端点选区保持选中。源码生成器、`.aaadaw` fixture、二进制 SHA-256 `798db8635dfdd57be454b391daf52b4ac63e909eb0e8839c38fa1f29728b1f13` / Build ID `4e073748e4df111213fdc2731ab7ce9c71ab1faa` 和截图哈希见 [AAADAW manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-program-aaadaw.tsv)。

Program 范围/Position/inversion 匹配、Set-from-selection、筛选列表、选择操作及工程快照回归通过；workspace Nextest **1,564/1,564，78 个测试目标，0 skipped**；AAADAW all-target strict Clippy（`-D warnings -A future-incompatible`）、Cranelift 桌面构建、fmt、diff check 和 fixture 哈希校验通过。Cranelift 已知 `TimelinePipeline: MaybeSync` future-incompatibility warning 仍被单独允许。截图验证行为和控件，不代表像素级一致验收。

`MIDI-EVENT-LIST-001`、UI-019 与 P7 继续 `in_progress`：混合事件类型与 Aftertouch、Channel Pressure、Meta、SysEx 的选区派生、其他筛选组合和完整窗口交互仍待处理。

### 2026-10-19 Program Change 隐式 Bank MSB

REAPER 7.82 的既有 Event List 实窗及 `MIDI_GetCC` readback 显示：没有此前同通道 CC0 的 Program Change 行仍以 `121-0-program` 显示参数。AAADAW 的 PC 参数列现仅在找不到此前同通道 CC0 时采用显示默认 MSB 121，已有 CC0/CC32 则继续显示各自生效值，不向工程插入 MIDI 事件。回归覆盖无 bank、显式 MSB 0 与显式 12-23 bank。

Cranelift 二进制 SHA-256 `1a2fbe880d5a6d4f875d974c943aa973b342af27f58a47518abd8eba3d7fd2e3` / Build ID `d3289d48f2cb43a095c7e26ae09f0928a3591584` 打开 Program Change fixture 后，在实际 MIDI Editor Event List 截图中显示 `121-0-8`、`121-0-32`、`121-0-64`，与原厂无 bank 行 `121-0-41` 对齐。截图、二进制身份、项目和 native 参照哈希登记在 [default-bank manifest](../verification/reaper-parity/fixtures/midi-event-list-program-change-default-bank.tsv)。workspace Nextest 1,564/1,564 与 AAADAW all-target strict Clippy 均通过。原厂 LSB-only bank 与同 tick 中 bank 事件位于 Program Change 之后的排序仍未单独探测；Event List 接受状态保持 `in_progress`。

### 2026-10-19 Poly Aftertouch Filter from selection

Linux REAPER 7.82 / Default 7 原厂探针选中 channel 2 的 note 60 / Pressure 32 @ tick 480 与 channel 3 的 note 72 / Pressure 100 @ tick 960，中间放置未选中的 note 64 / Pressure 64。`Set filter from selection` 推导 Poly Aftertouch、通道 2–3、Note C4–C5、Pressure 32–100 和 Position 0.125–0.25026042；Repeat every 保持 measure，绘制通道为 2，Event List 漏斗保持关闭。显式启用 Show-only 后只显示三条 Poly Aftertouch，未选中的中间事件也匹配。`MIDI_GetCC` 前后读回完全一致，RPP 的 `EVTFILTER` 保存全部筛选条件；脚本、RPP、readback 与截图见 [native fixture manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-poly-aftertouch-native.tsv)。

AAADAW 已实现相同的条件推导与 Event List 匹配；Poly Aftertouch 的 Note 范围、Pressure 范围、Position、通道、事件类型和 Invert 共同生效。Cranelift 实窗截图验证 Set 后的筛选对话框、保留两端选区的七行未筛选列表，以及启用 Show-only 后包含未选中中间事件的三行列表。项目生成器、`.aaadaw` fixture、截图和二进制身份见 [AAADAW fixture manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-poly-aftertouch-aaadaw.tsv)。Focused App 测试和消息层筛选测试通过；workspace Nextest **1,566/1,566，78 个测试目标，0 skipped**，AAADAW all-target strict Clippy（`-D warnings -A future-incompatible`）、fmt 和 diff check 均通过。

`MIDI-EVENT-LIST-001`、UI-019 与 P7 继续 `in_progress`：混合事件类型选区以及 Channel Pressure、Meta、SysEx 的 Set-from-selection 行为仍待原厂测量和实现；截图验证行为和控件，不代表像素级一致验收。


### 2026-10-19 Channel Pressure Filter from selection

Linux REAPER 7.82 / Default 7 原厂探针选中 channel 2 的 Pressure 32 @ tick 480 与 channel 3 的 Pressure 100 @ tick 960，中间放置未选中的 Pressure 64 @ tick 600。`Set filter from selection` 推导 Channel Pressure 类型、通道 2–3 和 Position 0.125–0.25026042；Repeat every 保持 measure，绘制通道为 2。可选 Pressure 数值范围维持未勾选、上下界为空，Set 后漏斗关闭。手动开启 Show-only 后，三条 Channel Pressure 均通过，中间未选中事件也显示。`MIDI_GetCC` 读回前后完全相同；脚本、RPP 和截图在 [native manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-channel-pressure-native.tsv)。

AAADAW 已实现一致的 Set-from-selection 默认条件，Event List 按 Pressure 条件（若启用）、Position、通道、类型和 Invert 匹配。Cranelift 实窗截图验证派生后的 Pressure 未勾选状态、七行未筛选 Event List，以及手动启用 Show-only 后三行 Channel Pressure 结果。可选 Pressure 范围由 App 回归覆盖，原厂手动启用该范围的匹配语义仍待测量；工程、截图和构建身份见 [AAADAW manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-channel-pressure-aaadaw.tsv)。

Channel Pressure 导出模型与筛选器回归通过；workspace Nextest **1,568/1,568，78 个测试目标，0 skipped**，AAADAW all-target strict Clippy（`-D warnings -A future-incompatible`）、fmt、diff check 与两份 fixture 哈希校验通过。Cranelift 已知 `TimelinePipeline: MaybeSync` future-incompatibility warning 仍单独允许。

`MIDI-EVENT-LIST-001`、UI-019 与 P7 继续 `in_progress`：其他混合类型组合、原厂手动 Pressure range、Meta 与 SysEx 的 Set-from-selection 行为仍待测量或实现；截图仅验证行为和控件，不代表像素级一致验收。

### 2026-10-19 Event List Filter 混合 Note+CC 选区

Linux REAPER 7.82 / Default 7 原厂工程选中 channel 2 的 C4 Note @ tick 480 与 channel 3 的 CC7 value 100 @ tick 960，并放置未选中的 channel 5 G4、channel 2 CC7/CC10、范围内 Text Meta @ tick 600，以及范围外 F0 SysEx @ tick 1500。`Set filter from selection` 将 Event type 设为 `<all events>`，只推导通道 2–3 与 Position 0.125–0.25026042，Repeat 为 measure，绘制通道为 2；Note、Velocity、Parameter、Value 和 Length 条件均关闭，Show-only 关闭。手动启用 Show-only 后恰有五行通过：三条匹配 CC、选中 Note，以及无通道的 Text Meta。通道掩码不排除无通道 Meta；Position 隐藏范围外 SysEx。G4 和终止行也隐藏。`MIDI_GetAllEvts` 前后 139 字节完全一致，RPP 保存相同 EVTFILTER；原厂脚本、读回、RPP 与三张实窗截图见 [native fixture](../verification/reaper-parity/fixtures/midi-event-filter-from-mixed-note-cc-native.tsv)。

AAADAW 的混合 channel-bearing 事件选择现采用相同公共条件：合并所选事件的通道和 Position 范围，将事件类型设为 All events，清空类型专属范围并把绘制通道设为最早选中事件的通道。opaque Event List 行也应用通用 Position 谓词。Cranelift 实窗确认八行未筛选表、Set 后的 All events / channels 2–3 / Position 对话框，以及显式启用筛选后的五行列表；Text Meta 通过，SysEx 隐藏。回归检查所选 Note/CC 保持选中，匹配结果、工程快照与 Undo/Redo 历史稳定；fixture、截图、二进制身份和逐项哈希见 [AAADAW manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-mixed-note-cc-aaadaw.tsv)。

workspace Nextest **1,570/1,570，78 个测试目标，0 skipped**；AAADAW all-target strict Clippy（`-D warnings -A future-incompatible`）、Cranelift 桌面构建、fmt、diff check 与 fixture 哈希校验通过。Nextest `--all-targets` 会把 render benchmark 的性能输出误解析为测试清单，因此完整回归使用 workspace 默认测试目标，排除 benchmark。其他混合类型组合和 Meta/SysEx 选中后 Set-from-selection 行为仍未核验，`MIDI-EVENT-LIST-001`、UI-019 与 P7 保持 `in_progress`；截图验证行为和控件，不代表像素级一致验收。

### 2026-10-19 MIDI Set Filter from Text Meta + SysEx

Linux REAPER 7.82 / Default 7 原厂探针选中 Text Meta @ tick 480 与 F0 SysEx @ tick 960，并以未选中的 CC 和 Note 作控制。`Set filter from selection` 将类型设为 `Sysex/Meta`、通道设为 All、Position 显示为 `1/8`–`0.250`、Repeat every 设为 measure，并自动启用 Show-only；Draw events on channel 保持 Channel 1。Event List 立即只显示仍保持选中的两条 opaque 行。`MIDI_GetAllEvts` 的 92 字节源数据前后 SHA-256 均为 `f7a0bb4f2c862b4dd4a142161277d894d10354b99f634544c796aa4e19300458`。原厂筛选状态由实窗截图确认；after-set RPP 未持久化所见 EVTFILTER。脚本、读回、字节数据、RPP 和截图在 [native manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-opaque-selection-native.tsv)。

AAADAW 现支持仅由 Text Meta 与 SysEx 组成的选区：推导 `Sysex/Meta`、All channels、事件 Position 范围，保持 Draw channel，并按原厂启用 Show-only。筛选面板现在也会为 `Sysex/Meta` 显示 Position 与 Repeat。Xorg :549 下的真实 Cranelift 窗口截图记录了五行未筛选列表、两条 opaque 选中行、派生筛选控件和两行结果；项目、生成器、截图与二进制 Build ID/SHA-256 在 [AAADAW manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-opaque-selection-aaadaw.tsv)。截图也暴露了尚存的显示差异：AAADAW 用八位小数的可编辑文本，REAPER 用组合框显示 `1/8` 和 `0.250`；原厂 after-set RPP 未保存筛选值，因此 opaque 选择的内部精确上界仍未独立读回。

新增的 Set-from-selection 与筛选控件回归通过；workspace Nextest **1,572/1,572，78 个测试目标，0 skipped**，断言加强后的两个定向回归 **2/2**；AAADAW all-target strict Clippy（`-D warnings -A future-incompatible`）、Cranelift 桌面构建、fmt 和 diff check 均通过。`MIDI-EVENT-LIST-001`、UI-019 和 P7 保持 `in_progress`：混合 channel-bearing/opaque 选区、更多异构组合、Position 控件形态/格式以及原厂窗口其他未覆盖交互仍待处理。


### 2026-10-19 MIDI Event List Filter 混合 CC + opaque 选区

Linux REAPER 7.82 / Default 7 原厂探针选中 channel 2 的 CC7 @ tick 480 与 Text Meta @ tick 960，同时放置未选中的 CC10、channel 5 Note、范围内 Text Meta、范围外 F0 SysEx 和终止行。`Set filter from selection` 将类型扩展为 `<all events>`、通道设为 All、按两个端点派生 Position `0.125–0.25026042`（控件显示 `1/8–0.250`），Repeat every 为 measure，自动启用 Show-only，并保持 Draw events on Channel 1。列表显示五条范围内事件，包含未选中的 CC、Note 和 Text Meta；范围外 SysEx 与终止行隐藏。两端选区保留，144 字节 `MIDI_GetAllEvts` 前后 SHA-256 相同；after-set RPP 的 `EVTFILTER` 保存了精确 Position 上界。脚本、readback、原始 MIDI、RPP 和 REAPER 实窗截图见 [native manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-mixed-opaque-selection-native.tsv)。

AAADAW 对该已测组合实现相同公共条件：All events / All channels、合并 Position、Repeat measure、Show-only 开启、Draw channel 保持 Channel 1；CC、Note 与无通道 Meta 按共同 Position 过滤。Cranelift 实窗截图记录七行原始列表、CC 与 Text Meta 两端选中、派生面板以及五行过滤结果。回归断言匹配事件、选区、工程快照与 Undo/Redo 历史；fixture、项目生成器、截图、Build ID/SHA-256 见 [AAADAW manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-mixed-opaque-selection-aaadaw.tsv)。

workspace Nextest **1,573/1,573，78 个测试目标，0 skipped**；定向回归 **1/1**；AAADAW all-target strict Clippy（`-D warnings -A future-incompatible`）、nightly Cranelift 桌面构建、fmt、diff check 均通过。两份 fixture manifest 的 **14/14** 个制品 SHA-256 匹配。REAPER 的 Position 组合框显示 `1/8–0.250`，AAADAW 仍使用可编辑小数文本 `0.12500000–0.25026042`；其他异构事件类型组合仍待测量和实现。`MIDI-EVENT-LIST-001`、UI-019 与 P7 保持 `in_progress`，截图验证行为和控件而非像素级一致。


### 2026-10-19 MIDI Event List Filter 混合 Note + opaque 选区

Linux REAPER 7.82 / Default 7 原厂探针选中 channel 2 的 C4 Note @ tick 480 与 Text Meta @ tick 960，并放置范围内 CC、未选 G4、Text Meta、范围外 SysEx 和终止行。Set 推导 `<all events>`、All channels、共同 Position `0.125–0.25026042`（界面显示 `1/8–0.250`）与 Repeat measure；Draw events on channel 保持 Channel 1。与 CC + opaque 组合不同，Show-only 维持未勾选，但 Event List 漏斗立即激活，因此五条范围内事件当场显示；关闭漏斗后恢复全部七行，Show-only 仍未勾选。选中 Note 和 Text Meta 保持选中，156 字节 `MIDI_GetAllEvts` 前后完全相同，RPP 保存了精确 Position 上界。ReaScript、readback、RPP、MIDI 字节及原厂实窗截图见 [native manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-mixed-note-opaque-selection-native.tsv)。

AAADAW 按该实测组合单独实现：通道和事件类型扩展为 All，派生共同 Position，开启 Event List funnel，但关闭 Show-only。Cranelift 实窗五张截图记录七行初始列表、两个选中端点、Set 面板、五行筛选结果及漏斗关闭后的七行恢复。App 回归覆盖所有匹配 Note/CC/opaque 事件、选区保留、漏斗切换、工程快照和 Undo/Redo 不变；项目生成器、fixture、截图和二进制身份见 [AAADAW manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-mixed-note-opaque-selection-aaadaw.tsv)。

workspace Nextest **1,574/1,574，78 个测试目标，0 skipped**；定向回归 **1/1**；AAADAW all-target strict Clippy（`-D warnings -A future-incompatible`）、nightly Cranelift 桌面构建、fmt 和 diff check 均通过。四份混合选择 manifest 的 **30/30** 个制品哈希匹配。CC + opaque 仍启用 Show-only，Note + opaque 则只激活漏斗；其余混合类型组合仍开放。Position 小数文本与 REAPER 组合框仍存在可见差异，因此 `MIDI-EVENT-LIST-001`、UI-019 与 P7 保持 `in_progress`；截图验证行为和控件，不代表像素级一致。


### 2026-10-19 MIDI Event List Filter 混合 Note + CC + Text Meta 选区

Linux REAPER 7.82 / Default 7 实测同时选中 C4 Note @ tick 480、CC7 @ tick 960 和 Text Meta @ tick 960：Set 推导 All events、All channels、共同 Position `1/8–0.250`、Repeat measure，勾选 Show-only 并启用漏斗；六行通过，关闭漏斗恢复八行，三个选中端点与 MIDI 数据保持不变。168 字节 `MIDI_GetAllEvts` before/after SHA-256 均为 `daf6aa01ef0e6c634f7e310febf9900da9191c9eb5c929ff19de250cca34e3e5`。筛选开启时 `MIDI_GetTextSysexEvt` 未返回范围外 SysEx；其保留由相同原始 MIDI 字节及 RPP before/after 支持。原厂探针、读回、RPP 和三张 REAPER 实窗截图见 [native manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-mixed-note-cc-opaque-selection-native.tsv)。

AAADAW 现在将精确的三类选区分支交给共享 mixed-opaque Position helper，自动派生共同 tick 范围并启用 Show-only；App 回归验证 All events / All channels、六个匹配事件、三个选区、漏斗关闭后的八行恢复以及 project/Undo/Redo 不变。Cranelift 桌面二进制 Build ID `8c9574dfe4ffef4d0472a1c4eecf80460d6f1e4e`、SHA-256 `d264a056ea9c1792785d047809a4b00e9a2590773b0ea5641cfccdbe69882c10` 的真实 Xvfb 窗口截图、fixture 和生成器见 [AAADAW manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-mixed-note-cc-opaque-selection-aaadaw.tsv)。定向回归 **1/1**、AAADAW package Nextest **831/831**、严格 all-target Clippy、nightly Cranelift 桌面构建、fmt、diff check 通过；两个本轮 manifest 的 **17/17** 个 artifact SHA-256 匹配。其他异构选区组合仍待测量与实现；Position 小数文本与 REAPER 组合框仍有界面差异，所以 `MIDI-EVENT-LIST-001`、UI-019 与 P7 保持 `in_progress`，截图验证行为和控件而非像素级一致。


### 2026-10-19 MIDI Event List Filter 混合 Note + Pitch Bend + Text Meta 选区

Linux REAPER 7.82 / Default 7 原厂探针选中 channel 2 的 C4 Note @ tick 480、channel 3 的 Pitch Bend +4096 @ tick 960，以及 Text Meta @ tick 960；控制项为范围内 CC7、G4、另一条 Text Meta、范围外 F0 SysEx 与终止行。`Set filter from selection` 推导 All events、All channels、Position `1/8–0.250` 和 Repeat measure，勾选 Show-only 并立即启用 Event List 漏斗；六行通过，关闭漏斗后恢复八行。三个所选端点保持选中，168 字节 `MIDI_GetAllEvts` before/after SHA-256 相同：`aa96adfd62802ee9d5509446fb02ee334759b0f9c026d5d7f5a8cbf66add0270`。REAPER 在过滤启用时没有通过 `MIDI_GetTextSysexEvt` 返回范围外 SysEx；原始 MIDI 字节与 before/after/funnel-off RPP 保留了该事件证据。[原厂 manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-mixed-note-pitch-opaque-selection-native.tsv) 收录 ReaScript、读回、三份 RPP 和实窗截图。

AAADAW 为该测量组合派生共同 Position 并启用 Show-only。App 回归通过 **1/1**，核对 All events / All channels、六个 Note/CC/Pitch Bend/Text Meta 匹配项、三个选中端点、漏斗关闭后八行恢复，以及工程快照与 Undo/Redo 历史不变。nightly Cranelift 桌面二进制 Build ID `0a233ec82d343c970ad1708aef5a2d9c418314c9`、SHA-256 `3af65dd98145d3d7f6039d09965dee3da735359986185be04bc178f0a19f6398` 的真实窗口截图、fixture 与生成器见 [AAADAW manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-mixed-note-pitch-opaque-selection-aaadaw.tsv)。AAADAW package Nextest **832/832**、严格 all-target Clippy、nightly Cranelift 桌面构建、fmt、diff check 和本组 manifest SHA-256 校验通过。REAPER 的 Position combo 显示 `1/8–0.250`，AAADAW 仍显示可编辑小数 `0.12500000–0.25026042`；未测量的异构组合仍开放，`MIDI-EVENT-LIST-001`、UI-019 与 P7 保持 `in_progress`。截图验证行为和控件，不代表像素级一致。


### 2026-10-19 MIDI Event List Filter 混合 Pitch Bend + Text Meta 选区

Linux REAPER 7.82 / Default 7 原厂 probe 选中 channel 3 的 Pitch Bend +4096 @ tick 480 与 Text Meta @ tick 960；控制项包含未选 C4、CC7、G4、范围内 Text Meta、范围外 F0 SysEx 及终止行。Set 推导 All events、All channels、共同 Position `1/8–0.250`、Repeat measure，勾选 Show-only 并启用 Event List 漏斗；六行通过，关闭漏斗恢复八行。两个选中端点保持选中，168 字节 `MIDI_GetAllEvts` before/after SHA-256 相同：`415cb5da07af0d22cbf5d9f77c487bf2707b0f1cd2fd4c8e86e59e8737c3fb2e`。滤镜开启时 ReaScript 未读回范围外 SysEx；原始 MIDI 字节及 before/after/funnel-off RPP 保留了该事件证据，详见[原厂 manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-pitch-opaque-selection-native.tsv)。

AAADAW 新增 Pitch Bend + opaque 分支，派生共同 Position 并启用 Show-only。App 回归 **1/1** 通过，覆盖 All events / All channels、六个匹配事件、两个选中端点、漏斗关闭后八行恢复，以及 project/Undo/Redo 不变。nightly Cranelift 桌面二进制 Build ID `84fd3e6db96f4e93bf121a591ef4ca3fb1234cb9`、SHA-256 `3594eacaf57a95c146496424d65276eeb75d9db1c8de4d4f0dd309d6052e2c90` 的真实 Xvfb 窗口截图、fixture 与生成器见 [AAADAW manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-pitch-opaque-selection-aaadaw.tsv)。AAADAW package Nextest **833/833**、严格 all-target Clippy、Cranelift 构建、fmt 与 diff check 通过；两份本组 manifest 的 artifact 哈希校验通过。Position 控件仍以小数输入框呈现，REAPER 则显示 `1/8–0.250`；更多异构组合仍开放，因此 `MIDI-EVENT-LIST-001`、UI-019 与 P7 保持 `in_progress`。截图验证行为和控件，不代表像素级一致。


2026-10-19 — 扩展 `Set filter from selection`，覆盖所选 CC + Pitch Bend + Text Meta：Linux REAPER 7.82 / Default 7 实测派生 All events、All channels、Position `1/8–0.250`、Repeat measure、Show-only；漏斗启用时显示 7 行、关闭后恢复 9 行。三类选中事件保持选中，180 字节原始 MIDI 前后逐字节相同（SHA-256 `bf7aeb087ea490f6709a8c32c973f6323536c03ba1b6a33b95a52c18ad43cdaa`）。AAADAW 已实现对应组合，真实 Cranelift 窗口与原版截图分别记录筛选前、派生面板、筛后及漏斗关闭状态；App 回归检查匹配 ID、选区与 project/Undo/Redo 不变。Focused App Nextest 1/1、AAADAW package Nextest 855/855、严格 all-target Clippy、nightly Cranelift 桌面构建、fmt/diff check、两份 fixture manifest 哈希校验均通过。详见 [原厂证据](../verification/reaper-parity/fixtures/midi-event-filter-from-cc-pitch-opaque-selection-native.tsv) 与 [AAADAW 证据](../verification/reaper-parity/fixtures/midi-event-filter-from-cc-pitch-opaque-selection-aaadaw.tsv)。此验收仍为 `in_progress`：Position 控件显示形式和其他异构组合尚未对齐。

2026-10-19 — 扩展 `Set filter from selection`，覆盖 Program Change + Text Meta。Linux REAPER 7.82 / Default 7 实测派生 All events、All channels、Position `1/8–0.250`、Repeat measure；Show-only 不勾选但 Event List 漏斗立即启用，筛出 5 行，关闭后恢复 8 行。Program Change 与 Text Meta 选区保持，154 字节原始 MIDI 前后完全相同（SHA-256 `67d57b92ac659fda9833a520b514ba1de41cdeacb54fa10c924ee651020eb7a9`）。AAADAW 实现同一行为，真实 Cranelift Event List 截图和 App 回归验证选区、过滤结果及 project/Undo/Redo 不变。Focused App Nextest 1/1、AAADAW package Nextest 856/856、严格 all-target Clippy、nightly Cranelift 构建、fmt/diff check 和原厂/AAADAW fixture 哈希校验通过；证据见 [原厂 manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-program-opaque-selection-native.tsv) 与 [AAADAW manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-program-opaque-selection-aaadaw.tsv)。Position 控件显示形式和其他异构组合仍未对齐，验收继续 `in_progress`。

2026-10-19 — 扩展 `Set filter from selection`，覆盖 Poly Aftertouch + Text Meta。Linux REAPER 7.82 / Default 7 原厂实测派生 All events、All channels、Position `1/8–0.250`、Repeat measure；Show-only 保持关闭，Event List 漏斗立即启用并显示 5 行，关闭后恢复 8 行。两个选中端点保持选中，156 字节原始 MIDI 前后完全相同（SHA-256 `db268d5a71ce0d10184809d9bd920d725670a9479a37deb0db8e7946cebc35b1`）。AAADAW 实现同一行为，Cranelift 实窗截图及 App 回归覆盖筛选、选区和 project/Undo/Redo 不变。Focused App Nextest 1/1、package Nextest 857/857、严格 all-target Clippy、nightly Cranelift 构建、fmt/diff check、原厂和 AAADAW manifest 哈希校验通过；详见 [原厂证据](../verification/reaper-parity/fixtures/midi-event-filter-from-poly-aftertouch-opaque-selection-native.tsv) 与 [AAADAW 证据](../verification/reaper-parity/fixtures/midi-event-filter-from-poly-aftertouch-opaque-selection-aaadaw.tsv)。Position 控件显示形式和其余异构组合未完成，验收继续 `in_progress`。
2026-10-19 — 完成 Channel Pressure + Text Meta 异构选择覆盖。Linux REAPER 7.82 / Default 7 实测 Set 后为 All events、All channels、Position `1/8–0.250`、Repeat measure、Show-only 关闭且漏斗启用；通过 5 行，关漏斗恢复 8 行，选区保持，154 字节 MIDI 原始数据前后哈希均为 `66ab71ce…e18ff`。AAADAW 已实现同一共同 Position 过滤，回归覆盖选区、5/8 行、project/Undo/Redo 不变；native 和 Cranelift 实窗截图及原始读回证据在两份 fixture manifest。Focused App Nextest 1/1、AAADAW package Nextest 858/858、nightly Cranelift 构建通过。Position 控件展示差异和其他异构组合仍未完成，验收保持 `in_progress`。


### 2026-10-19 MIDI Filter：Program Change + Channel Pressure + Text Meta

Linux REAPER 7.82 / Default 7 在 Show-only 初始未勾选时，选中 channel 2 的 Channel Pressure value 32 @ tick 480、Program Change 8 @ tick 600 和 Text Meta @ tick 960。`Set filter from selection` 推导 All events、All channels、Position `1/8–0.250`（RPP 保存 `0.125–0.25026042`）、Repeat measure，并同时开启 Show-only 与 Event List 漏斗；七行通过。关闭漏斗后恢复十行，并清除 Show-only，保留 Position。三类选区保持，176 字节 `MIDI_GetAllEvts` 前后完全一致（SHA-256 `90106f9c4d87059acd72c623094ef5aaae4b0ff7429129d88a1381a5bab0a813`）。原厂探针、读回、字节流、RPP 与 Default 7 实窗截图见 [native manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-program-channel-pressure-opaque-selection-native.tsv)。

AAADAW 现按相同条件派生筛选并正确勾选 Show-only。修正后的 Cranelift 桌面二进制 Build ID `4b0efd847d74dd25a4449a1806d43b16050f8c6b`、SHA-256 `9d064c15f1e082a66da635b0d38199ed1da0afef15f378cb18bcd91d07d06e39` 已在 Linux Xvfb 实窗逐项验证：三行选区正确保持，筛后七行，关漏斗后十行且 Show-only 清除。App 回归核对 PC、Channel Pressure、CC、opaque 匹配集合与工程快照/Undo/Redo 不变。AAADAW package Nextest **859/859**，严格 all-target Clippy、nightly Cranelift 构建、fmt、diff check 和两份 fixture manifest 的 **21 项 SHA-256** 校验通过。Position 仍以可编辑十进制文本呈现，而 REAPER 使用显示 `1/8–0.250` 的组合框；其他异构组合未全部覆盖，`MIDI-EVENT-LIST-001`、UI-019 与 P7 继续 `in_progress`。


### 2026-10-19 MIDI Filter：Program Change + Poly Aftertouch + Text Meta

Linux REAPER 7.82 / Default 7 实测选中 channel 2 的 Poly Aftertouch pressure 32 @ tick 480、Program Change 8 @ tick 600，以及 Text Meta @ tick 960。Set 推导 All events、All channels、Position `1/8–0.250`（RPP 为 `0.125–0.25026042`）、Repeat measure，并勾选 Show-only、启用 Event List 漏斗；筛后七行，关闭漏斗恢复十行并清除 Show-only。三类选区保持，178 字节 `MIDI_GetAllEvts` before/after 完全相同，SHA-256 `8ee9b3a147d4ae76356d502378a8a79cc5d45af7a2b4421016b613d9dc3b19c6`。原厂探针、读回、RPP 与 Default 7 实窗五状态截图见 [native manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-program-poly-aftertouch-opaque-selection-native.tsv)。

AAADAW 原先从该三类混合选择求 Position 时遗漏所选 Poly Aftertouch tick。本次增加该组合的原生分支，并按实测启用 Show-only 与漏斗；App 回归验证七个匹配事件、三类选择保持、漏斗关闭后十行恢复，以及项目快照和 Undo/Redo 不变。Cranelift 桌面 Build ID `82dbfb5d98d2ab19332ccb79f0580847afb2e3eb`、SHA-256 `fdf6a6c947f56c75e1f04ff1ab446cefe3edf7608696ffa5c9c4cf695a81cc8c` 的实际 MIDI Event List 五状态截图、fixture 和生成器见 [AAADAW manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-program-poly-aftertouch-opaque-selection-aaadaw.tsv)。聚焦回归 **1/1**、AAADAW package Nextest **860/860**、严格 all-target Clippy、nightly Cranelift 构建、fmt/diff check 及两份 manifest 的 **21/21** artifact SHA-256 校验通过。Position 小数字段与 REAPER 组合框仍有界面差异，其他异构组合也未完成，`MIDI-EVENT-LIST-001`、UI-019 与 P7 继续 `in_progress`；截图验证行为和控件，不代表像素级一致。


### 2026-10-19 MIDI Filter：Program Change + Poly Aftertouch + Channel Pressure + Text Meta

Linux REAPER 7.82 / Default 7 在 Show-only 初始关闭时，选择 channel 2 的 Poly Aftertouch @ tick 480、Program Change @ tick 600、Channel Pressure @ tick 720 和 Text Meta @ tick 960。Set 推导 All events、All channels、Position `1/8–0.250`（RPP 保存 `0.125–0.25026042`）与 Repeat measure；Show-only 保持关闭，Event List 漏斗开启，筛出 8 行，关闭漏斗恢复 11 行。四个选中端点保持选中，189 字节 `MIDI_GetAllEvts` 前后相同，SHA-256 `831499a22229cf1f9a71b6966f28703b7c064bdd6e29dd972c35dd7ca170c900`。原厂探针、三份读回/RPP、原始 MIDI 与 Default 7 实窗截图见 [native manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-program-poly-channel-pressure-opaque-selection-native.tsv)。

AAADAW 增加该精确四类组合的 Position 派生分支；实窗复核确认同一范围、Show-only 关闭、筛后八行、关漏斗后十一行和选区保留。App 回归也核对八个事件身份、四类选区、项目快照及 Undo/Redo 不变。nightly Cranelift Build ID `dec6dcbf90f62c82f7ab1f88367e985cc18e621a`、SHA-256 `d4c9b2bb674f513651d1aece26d26469ad45756ba66b2773b626714ff9ddc18d` 的六张真实 Event List 截图和 fixture 见 [AAADAW manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-program-poly-channel-pressure-opaque-selection-aaadaw.tsv)。该类型组合现已覆盖；Position 小数文本与 REAPER 组合框的界面差异及其他异构组合仍未完成，`MIDI-EVENT-LIST-001`、UI-019 和 P7 保持 `in_progress`。截图验证行为和控件，不代表像素级一致。

### 2026-10-19 MIDI Filter：CC + Program Change + Text Meta

Linux REAPER 7.82 / Default 7 原厂探针选中 channel 2 的 CC7/value 64 @ tick 480、Program Change 8 @ tick 600 与 Text Meta @ tick 960。Set 推导 All events、All channels、Position `1/8–0.250`（RPP 保存 `0.125–0.25026042`）和 Repeat measure；Show-only 保持关闭，Event List 漏斗启用。筛后九行通过，关闭漏斗恢复十二行；三类选区保持。201 字节 `MIDI_GetAllEvts` 在 Set 前后完全相同，SHA-256 为 `09184bd7df2c56f42441e7d8e6a9cc5cd2246b9aa55c4857706fbbf359abfe9f`。原厂 probe、读回、三份 RPP、MIDI 字节和四张 Default 7 实窗截图见 [native manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-cc-program-opaque-selection-native.tsv)。

AAADAW 增加相同组合的共同 Position 派生，并匹配 Show-only 与漏斗状态。App 回归验证九个命中事件、三类选区、漏斗关闭后十二行恢复，以及项目快照和 Undo/Redo 不变。重新启动的 Cranelift 桌面程序 Build ID `536ce8bfe8e1289dff77abc302d77f784c955018`、SHA-256 `b8c79eab7de0f409a1d1b59d3334d7a0dbfcc8cbd264a6be45ace47e8bc4faff` 在真实 Event List 窗口采集六个状态；fixture 与构建身份见 [AAADAW manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-cc-program-opaque-selection-aaadaw.tsv)。Focused App Nextest **1/1**、package Nextest **862/862**、严格 all-target Clippy、nightly Cranelift 构建、fmt/diff check 与两份 manifest **21/21** artifact 哈希检查通过。Position 仍显示可编辑小数而 REAPER 用 `1/8–0.250` 组合框；其他异构组合仍待测，`MIDI-EVENT-LIST-001`、UI-019 与 P7 继续 `in_progress`。截图验证行为和控件，不代表像素级一致。


### 2026-10-19 MIDI Filter：CC + Program Change + Poly Aftertouch + Text Meta

Linux REAPER 7.82 / Default 7 选中 channel 2 的 CC7/value 64 @ tick 480、Program Change 8 @ tick 600、Poly Aftertouch C4/pressure 32 @ tick 480 与 Text Meta @ tick 960。Set 推导 All events、All channels、Position `1/8–0.250`（RPP 保存 `0.125–0.25026042`）及 Repeat measure；Show-only 和 Event List 漏斗都保持关闭。手动开漏斗显示 9/12 行，关闭后恢复 12 行；四个端点保持选中。201 字节 MIDI 原始数据 Set 前后相同，SHA-256 `5d380f985dda89a2c922b56b7af8d983e63e97e1427f51b32e06ae9bb410fc7f`。原厂脚本、三份读回/RPP、字节流及 Default 7 实窗截图见 [native manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-cc-program-poly-aftertouch-opaque-selection-native.tsv)。

AAADAW 已实现实测行为；回归分别保留旧 CC + Program Change + opaque 组合和新增四类组合，覆盖漏斗默认值、9/12 行变化、选区及 project/Undo/Redo 不变。Build ID `321b06c6f0a08f816c13e13f5864ccc6f239e9d1`、SHA-256 `1ea296cd81b50a6fb663fd43e48414ba75c7a8826ccf0519594d33dfe6c1f876` 的 Cranelift 二进制已在真实 MIDI Event List 窗口复核；六张交互截图及 fixture 见 [AAADAW manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-cc-program-poly-aftertouch-opaque-selection-aaadaw.tsv)。两条聚焦回归 **2/2**、AAADAW package Nextest **863/863**、严格 all-target Clippy、nightly Cranelift 构建、fmt/diff check 和两份 manifest **21/21** artifact 哈希检查通过。Position 可编辑小数与 REAPER 组合框仍有显示差异，更多异构组合尚未穷尽，`MIDI-EVENT-LIST-001`、UI-019 和 P7 继续 `in_progress`；截图验证行为和控件，不代表像素级一致。


### 2026-10-19 MIDI Filter：CC + Channel Pressure + Text Meta

Linux REAPER 7.82 / Default 7 实测选中 channel 2 的 CC7/value 64 @ tick 480、Channel Pressure/value 32 @ tick 600 和 Text Meta @ tick 960。Set 推导 All events、All channels、Position `1/8–0.250`（RPP 保存 `0.125–0.25026042`）、Repeat measure；Show-only 与 Event List 漏斗都保持关闭。手动开漏斗显示六行，关闭后恢复九行，三个端点仍选中。166 字节 `MIDI_GetAllEvts` 前后相同，SHA-256 `b1fe0b7c5294047ad98ec935a329d8608d6053cbb720be00642d293ae1bed329`；原厂读回、RPP、字节流和 Default 7 实窗截图见 [native manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-cc-channel-pressure-opaque-selection-native.tsv)。

AAADAW 实现相同条件，App 回归核对九个匹配事件、三项保留选区、漏斗开启时六行、关闭后九行，以及项目快照和 Undo/Redo 历史不变。Cranelift 桌面 Build ID `a321ac629bd3fdd478493185c68ba04c8cfccb6e`、SHA-256 `3fb38f770b059681db0df8cba25924a58b66811e03574cf55136688fe3d56d13` 已在实际 MIDI Event List 窗口验证；六张截图与 fixture 见 [AAADAW manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-cc-channel-pressure-opaque-selection-aaadaw.tsv)。聚焦回归 **2/2**、AAADAW package Nextest **864/864**、严格 all-target Clippy、nightly Cranelift 桌面构建、fmt/diff check 和两份 manifest 的 **24/24** artifact SHA-256 检查通过。Position 仍是可编辑小数文本而 REAPER 显示 `1/8–0.250` 组合框，未覆盖的异构组合仍开放，`MIDI-EVENT-LIST-001`、UI-019 与 P7 保持 `in_progress`；截图验证行为和控件，不代表像素级一致。


### 2026-10-19 MIDI Filter：CC + Program Change + Channel Pressure + Text Meta

Linux REAPER 7.82 / Default 7 实测这四类事件混选：channel 2 的 CC7/value 64 @ tick 480、Program Change 8 @ tick 600、Channel Pressure/value 32 @ tick 720 和 Text Meta @ tick 960。Set 推导 All events、All channels、Position `1/8–0.250`（RPP `0.125–0.25026042`）和 Repeat measure；Show-only 与 Event List 漏斗均关闭。手动开漏斗显示 8/11 行，再关闭恢复 11 行，四个端点的选择保留。188 字节 `MIDI_GetAllEvts` 前后 SHA-256 相同：`2e9764c07419db3bf1d0591a736b1b17bdba3a46dcc8cfed68aa1aa7655dd7a9`。REAPER 原厂读回、四个 RPP 状态、两份 MIDI 原始数据和五张 Default 7 实窗截图见 [native manifest](../verification/reaper-parity/fixtures/midi-event-filter-from-cc-program-channel-pressure-opaque-selection-native.tsv)。

AAADAW 已实现该组合；App 回归检查精确的八个匹配事件、四个选中端点、11 行恢复，以及 project/Undo/Redo 状态不变。新 Cranelift 桌面二进制 Build ID `8ac5bef58971e412316b8193f96261c3d5a1b7e9`、SHA-256 `ca6184d790bc7efdcfa40ebbf369d230c10b4d3a161e385a2059c922d56c544f` 已在真实 MIDI Event List 窗口完成交互截图核验。AAADAW Nextest **865/865**、严格 all-target Clippy、nightly Cranelift 构建、fmt/diff check 与两份 manifest 的 **24/24** artifact SHA-256 校验通过。Position 编辑字段仍与 REAPER 的 `1/8–0.250` 组合框显示不同；未覆盖的异构组合和 Event List 交互继续保持开放，`MIDI-EVENT-LIST-001`、UI-019 与 P7 仍为 `in_progress`，截图用于行为核验而非像素一致性验收。

### 2026-10-19 P5 Take: Paste as takes in items

Linux REAPER 7.82 / Default 7 原厂探针确认 Main action 40603 将所选源项的当前 Take 追加到目标项末尾并激活：目标原 Take 保留为 index 0，粘贴 Take 成为 active index 1，来源项仍留在轨道上，目标位置/长度不变；一次 Undo 移除新 Take，Redo 恢复并激活。Main KeyMap 的 Copy items/tracks 与 Paste as takes 映射使用 40057/40603。

AAADAW 已接入单个选中音频项的活动 Take 复制，以及向一个选中音频目标项追加并激活；Take 的媒体引用、名称、source view、极性、loop、gain、pan、playback rate 随拷贝保留，工程撤销历史以单步提交。REAPER 和 AAADAW 的实时 Xvfb 窗口截图、native RPP 状态、可复现 capture script 与 fixture hash 见 [Paste as takes 证据](../verification/reaper-parity/audio-paste-as-takes.md)。Stable AAADAW desktop suite 846/846、Core/Storage 371/371 通过。`EDIT-TAKE-001` 保持 `in_progress`：lane 显示、录音生成、Comp、Take 管理、多选及跨工程剪贴板仍未实现。


### 2026-10-19 P5 Take: Delete active take from items

Linux REAPER 7.82 / Default 7 原厂 action 40129 `Take: Delete active take from items` 无默认快捷键。删除双 Take 项目的活动末项后，前一项成为 active index 0；删除首项后，后续项成为 active index 0；只有一个 Take 时整个 Item 被删除。三个行为的 Undo/Redo 与保存的 RPP 读回均已核对，原厂 Arrange 截图显示删除 Take A 后保留 Take B。

AAADAW Core 新增单步、可撤销的活动 Take 删除动作：保留其他 Take 并按原厂顺序激活；单 Take 时移除 Item。Item 菜单与 Action List 注册 40129，Inspector 为多 Take 项目提供删除按钮；冻结渲染项禁用编辑，音频图在正常 UI 图重建路径切换源。Stable 桌面回归 **847/847**、Core/Storage **373/373**、严格 all-target Clippy、fmt 与 `git diff --check` 通过。AAADAW Stable LLVM 实窗删除前后截图及原厂读回见[删除活动 Take 证据](../verification/reaper-parity/audio-delete-active-take.md)。`EDIT-TAKE-001` 仍 `in_progress`：Lane/Comp、录音生成、Take 重排、多选、MIDI Item Take 与跨工程剪贴板仍未覆盖。

### 2026-10-19 P5 Take: 多选 Crop to active take

Linux REAPER 7.82 / Default 7 原厂 action 40131 对选中的每个多 Take 音频 Item 分别保留活动源，选中的单 Take Item 不变；一次 Undo 恢复全部多 Take 状态，一次 Redo 重做全部裁切。Take A 在 index 1、Take B 在 index 0 的两个项目及单 Take 项的 RPP before/after/undo/redo 读回和 Arrange 截图已归档。

AAADAW 将 40131 扩展为一条批量事务：Action List、Item 菜单和 Inspector 都作用于选中的多 Take 音频项；混合选择内的单 Take 项无变化，任何裁切目标若为冻结渲染则整批拒绝。稳定 Stable LLVM 窗口的四张 1280×800 截图记录 Inspector 控件、裁切、单步 Undo 与 Redo；自包含工程、生成器、原厂 RPP 与状态读回见[多选裁切 manifest](../verification/reaper-parity/fixtures/audio-crop-multi-selection-aaadaw.tsv)。Stable Nextest **1246/1246**、严格 all-target Clippy、fmt、capture shell 语法、diff 检查及 parity matrix **113 条 / 15 列**校验通过，本组 manifest **17/17** 制品哈希匹配。`EDIT-TAKE-001` 继续 `in_progress`：其他 Take 操作的多选、录音生成、Lane/Comp、MIDI Item Take 与跨工程剪贴板仍开放。

### 2026-10-19 P5 Take: 多选 Move active takes to top

Linux REAPER 7.82 / Default 7 原厂 action 41380 将选中的两个 active index-1 Take 项同时移至 index 0；单 Take 项和已在顶部的多 Take 项不变。真实 Ctrl+Z 一步恢复两个顺序，Ctrl+Shift+Z 重做；再次运行原厂动作不改变撤销描述。Before/after/undo/redo/already-top 的 RPP、Arrange 截图和状态读回见[多选重排 manifest](../verification/reaper-parity/fixtures/audio-move-active-take-to-top-multi-aaadaw.tsv)。

AAADAW 的 Action List、Item 菜单和 Inspector 现对选中多 Take 音频项按项目顺序执行原子批事务；单 Take/已在顶部项目不变，冻结渲染目标会阻止整批修改。Stable LLVM 实窗四态截图显示 Inspector 操作、单步 Undo 与 Redo；回归核对来源顺序、混合选择和单一历史步。Stable Nextest **1247/1247**、严格 all-target Clippy、fmt、capture shell 语法和 diff 检查通过。`EDIT-TAKE-001` 保持 `in_progress`：其他 Take 操作的多选、录音生成、Lane/Comp、MIDI Item Take 与跨工程剪贴板仍开放。

### 2026-10-19 P5 Take: 多选 Delete active take

Linux REAPER 7.82 / Default 7 原厂 action 40129 会处理全部选中 Audio Item：首位与末位活动 Take 均删除后保留另一个 Take 于 index 0；单 Take 项会被删除；一次 Undo 恢复三项及原活动索引，一次 Redo 重做整组。原厂状态读回、RPP 与 Arrange 截图已归档。

AAADAW 现让 Action List、Item 菜单和 Inspector 的删除动作覆盖所有选中音频项；多目标通过一个 `BatchTransaction` 提交，冻结渲染目标阻止整批操作，播放图在删除或活动源变化后按原有安全路径重建。独立 Stable 实窗四态截图覆盖删除、Undo、Redo；自动化回归验证两种活动索引、单 Take 项删除、单历史步与工程快照恢复。Stable Nextest **1606/1606**、严格 all-target Clippy、fmt、capture shell 语法、diff 检查、parity matrix **113 条 / 15 列**与 manifest **17/17 哈希**均通过。证据与哈希见[多选删除 manifest](../verification/reaper-parity/fixtures/audio-delete-active-take-multi-aaadaw.tsv)。`EDIT-TAKE-001` 继续 `in_progress`：Copy/Paste 和其他 Take 操作的多选、录音生成、Lane/Comp、MIDI Item Take 与跨工程剪贴板仍开放。

### 2026-10-19 P5 Take: 多选 Duplicate active take

Linux REAPER 7.82 / Default 7 原厂 action 40639 将每个选中 Audio Item 的活动 Take 副本追加到列表末尾并激活它；混合选择中的单 Take 项也变成双 Take。对首项活动索引 0、单 Take 项、末项活动索引 1 的组合，Take 数量从 2/1/2 变为 3/2/3，新增副本保留原活动源，顺序与原厂读回一致；真实 Ctrl+Z 一步恢复整组，Ctrl+Shift+Z 一步重做。原厂 Arrange 截图左侧被评估弹窗部分遮挡，因此状态 TSV 和 before/after/undo/redo RPP 为主要证据，见[多选复制 manifest](../verification/reaper-parity/fixtures/audio-duplicate-active-take-multi-aaadaw.tsv)。这次原厂测量也纠正了早先单项探针的错误推断：旧探针在 action 后手动改过 Take 源，不能单独证明副本插入位置。

AAADAW Core 已按实测改为 append + activate；Action List、Item 菜单和 Inspector 对选中音频项执行一条 `BatchTransaction`，包含单 Take 项，冻结目标拒绝整组操作，忙碌状态和单项 Inspector 路径也受保护。真实 Linux Xvfb 窗口以 nightly Cranelift 桌面二进制完成四态截图，显示选中操作、Take 3/3、单步 Undo/Redo；该 capture build 没有启用 audio-device，所以截图验证可视编辑状态，声音播放不在本次验收范围。Stable LLVM Nextest **1608/1608**、严格 all-target Clippy、fmt、capture shell 语法、diff 检查和 parity matrix **113 条 / 15 列**与本组 manifest **19/19 哈希**通过；nightly Cranelift 桌面构建成功，同时输出既有 recursion-depth future-incompatible warning。`EDIT-TAKE-001` 继续 `in_progress`：多选 Paste as takes、选中 Take 切换、录音生成、Lane/Comp、MIDI Item Take、完整 Item Properties 与跨工程剪贴板仍开放。

### 2026-10-19 P5 Take: 多选 Paste as takes

Linux REAPER 7.82 / Default 7 action 40603 对选中的全部 Audio Item 追加同一个剪贴板活动 Take 并激活副本；单 Take 目标也参与，原剪贴板来源 Item 仍留在 Arrange。首 Take 活动索引 0、单 Take、末 Take 活动索引 1 的三个目标从 Take 数量 2/1/2 变为 3/2/3，新增来源均在列表末尾并成为活动 Take；目标位置/长度未变化。真实 Ctrl+Z 一步恢复所有 Take 列表，Ctrl+Shift+Z 重做。原厂四个 RPP 状态读回和清理浮动窗口后的 1280×800 Arrange 截图见[多选 Paste as takes manifest](../verification/reaper-parity/fixtures/audio-paste-as-takes-multi-aaadaw.tsv)。

AAADAW 现将同一个项目内的剪贴板活动 Take 以一条 `BatchTransaction` 追加到选中的每个音频目标；单选仍走单个 Core action，混合 Audio/MIDI 选择只处理 Audio Item，任一冻结目标使整个操作不可用/拒绝。Inspector 增加可用的 **Paste as takes** 按钮。nightly Cranelift 二进制实窗四态截图显示来源项保留、三项目标变化、按钮状态和单步 Undo/Redo；测试构建未启用 audio-device，因此截图不验证播放声音。Stable LLVM Nextest **1609/1609**、严格 all-target Clippy、fmt、capture shell 语法和 diff 检查通过；nightly Cranelift 桌面构建与实际截图成功，仍产生 recursion-depth future-incompatible warning；parity matrix **113 条 / 15 列**与本组 manifest **19/19 哈希**核对通过。`EDIT-TAKE-001` 继续 `in_progress`：多来源 Clipboard、跨工程媒体、MIDI/Track Copy、录音 Take、Lane/Comp、Take 重排以外的 Take 管理仍开放。

### 2026-10-19 P5 Take: 多选 Next/Previous take

Linux REAPER 7.82 / Default 7 原厂 action 40125/40126 对选中的多 Take 项采用共享 Take 槽位。Take 数量 2/1/3、起始索引 0/0/2 时，Next 得到 0/0/0，单 Take 项不变；一次 Undo/Redo 撤销或重做整组，随后 Previous 得到 1/0/2。混合索引对照从 1/0/0 开始，Next 得到 1/0/2，紧接 Previous 得到 1/0/1。读回表、所有 RPP 状态和 Linux REAPER 实窗截图见[多选切换 manifest](../verification/reaper-parity/fixtures/audio-switch-takes-multi-aaadaw.tsv)。

AAADAW Action List、Item 菜单和多选 Inspector 的 Next takes / Previous takes 按钮现按同一共享槽位规则切换；较短的多 Take 项限制在其最后一个 Take，单 Take 和 MIDI 项不变，冻结目标阻止整批修改。所有选中目标由一条 `BatchTransaction` 记录，单次 Undo/Redo 恢复完整快照。nightly Cranelift 二进制在 Linux Xvfb 实窗完成按钮操作和四态截图，显示活动 Take、状态栏反馈及撤销重做。

Stable LLVM workspace Nextest **1610/1610**、AAADAW 桌面单元测试 **855/855**、严格 all-target Clippy、fmt、capture shell 语法、diff 检查、parity matrix **113 条 / 15 列**及本组 manifest SHA-256 核对均通过。Cranelift 构建报告 Build ID `fe30f3cfd39b6b47203fe75bf5479ae5e359c811`，保留既有 recursion-depth future-incompatible warning。该构建禁用了 audio-device，实窗截图验证界面和 Take 状态，不代表播放声音已验收。`EDIT-TAKE-001` 仍 `in_progress`：录音生成、更多剪贴板来源、跨工程剪贴板、Lane/Comp、MIDI/Track Copy、完整 Media Item Properties 和其他 Take 操作仍开放。

### 2026-10-19 P5 Take / Item Properties: Active take selector

Linux REAPER 7.82 / Default 7 的 Media Item Properties 原厂截图显示 **Active take** 列表按顺序列出 `1: Take B`、`2: Take A`，并标出当前 Take A。AAADAW 的 F2 Media Item Properties 增加相同编号与名称的选择器；选择项目直接提交 `SetAudioItemActiveTake`，同步刷新名称、速率和其他当前 Take 字段。播放/工程操作忙碌或冻结 Render Item 时拒绝修改；Undo、Redo 和其他切换动作都会更新仍打开的属性窗。

Stable AAADAW 桌面测试 **856/856**、新增属性选择器与开窗撤销/重做回归、严格 all-target Clippy、fmt 与 diff 检查通过。nightly Cranelift Linux X11 二进制在隔离 Xvfb 1280×800 实窗中捕获初始属性、展开的 Take 列表、切换到 Take B、Undo 回到 Take A、Redo 回到 Take B；构建 Build ID `a53df722964459cbfc1f39f22ee2160919d76d0a`，SHA-256 `d6409a78a2ece6fae18e8c8e587b56b1af276d2ea782e605efab6f4987e3ada6`。原厂与 AAADAW 截图、测试和文件哈希见[Active take 属性选择器 manifest](../verification/reaper-parity/fixtures/audio-take-properties-selector-aaadaw.tsv)。此截图构建未启用 audio-device；只核验窗口和状态，不核验声音。`ITEM-PROPERTIES-001` 与 `EDIT-TAKE-001` 保持 `in_progress`，尚有完整原厂窗口几何、其他属性字段、录音 Take、固定 Lanes 和 Comp 等未完成范围。

### 2026-10-19 P5 Take: 多来源 Paste as takes

Linux REAPER 7.82 / Default 7 实测 Main action 40603：复制两个选中的音频源 Item 后，两个来源按项目顺序都追加到每个选中的目标 Item；最后追加的 Source B 激活。仅选一个目标时仍会追加两份来源。目标位置/长度与来源 Item 均不变；两目标操作一次 Undo/Redo 覆盖全部目标。读回表和五份 RPP 状态见[原厂 manifest](../verification/reaper-parity/fixtures/audio-paste-as-takes-multiple-sources-aaadaw.tsv)。

AAADAW 的 Copy items 现按项目顺序保存所有选中音频项的活动 Take；Paste as takes 将完整顺序应用到每个选中音频目标，单来源广播路径维持原行为，多个来源与多个目标在一个 `BatchTransaction` 内提交。回归覆盖 2→3 Take 的双目标、多来源进入单目标、来源保留、单次历史事务与完整 Undo/Redo 快照。真实 Cranelift AAADAW 窗口四态截图显示两个目标的 3/3 活动 Source B、来源 Item、撤销和重做；原厂与 AAADAW 工程、探针脚本和 SHA-256 见[多来源 Paste as takes manifest](../verification/reaper-parity/fixtures/audio-paste-as-takes-multiple-sources-aaadaw.tsv)。

Stable LLVM AAADAW 包测试 **1612/1612**（79 个 test binary，0 skipped）、严格 all-target Clippy、fmt、capture shell 语法、diff check、parity matrix **113 行 / 15 列**及 manifest 制品哈希审计通过。nightly Cranelift 桌面构建成功，保留既有 recursion-depth future-incompatible warning；Xvfb 实窗未启用 audio-device，因此截图只核验界面与 Take 状态。`EDIT-TAKE-001` 仍 `in_progress`：录音生成、跨工程剪贴板、MIDI/Track Copy、其他 Take 操作、Show all takes、Fixed Lanes 与 Comp Areas 尚未完成。
