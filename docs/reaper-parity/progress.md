# 对齐进度

## 会话记录

| 字段 | 状态 |
| --- | --- |
| 用户批准 | 2026-10-09；Linux / REAPER 7.82 / 默认主题与配置 |
| 调研提交 | `8cff888a44b67590f0969a3ece82d4ad9e9e0dba` |
| 唯一开发分支 | `feat/reaper-parity` |
| 唯一 PR | [#282](https://github.com/mumu-lhl/AAADAW/pull/282)，Draft；后续阶段只更新此 PR |
| 当前交付 | 唯一开发分支与 Draft PR #282 持续推进；P4 已提交 Contents Nudge 的原厂边界与 EOF 静音证据，并已接入 schema 49 有符号 source offset、负偏移 pre-roll/seek、持久化、属性编辑与波形显示；looped MIDI Contents Nudge 已贯穿 source-loop 模型，Snap 开/关一致性已按原厂 probe 修正，Audio、混合 Audio/looped-MIDI 与 non-loop MIDI 的绝对 Set Contents 已有原厂对照和单事务 Undo/Redo；non-loop MIDI Contents Nudge 仍待实现；已有 P10 纵向切片覆盖 Project Settings Notes、General BPM/拍号/采样率、有符号 Start measure、BPM basis、标尺间隔锚点、Project start time 显示偏移、受支持字段的默认工程设置保存、64-tap sinc 播放、192-tap sinc 离线渲染、Entire project/Time selection/Custom time range/All project regions/All project markers 范围、默认关闭的毫秒尾音控制、Project/常用预设输出采样率转换、Mono/Stereo WAV、PCM8/16/24/32、Float32/64、4-bit IMA ADPCM、2-bit cADPCM 与 8-bit u-Law、可选 sample-peak 归一化（−60 至 0 dBFS）、Selected tracks (stems)、pre-fader stems、Selected tracks via master 与 Master mix and stems；另有 Project Marker 稳定身份/可见编号、schema36 保存/迁移、插入/导航/删除/快捷动作/拖动/属性编辑；Loop Ruler 已补充 Shift 整段移动/Snap、底栏建点、项目级 Loop/Time Selection 保存恢复、全局 link 偏好持久化、Action List link on/off 状态与 40632/40633 范围导航；cADPCM 编码、Repeat 播放、其他 P4/P6 和完整原厂验收仍未完成 |
| 产品覆盖率 | 尚不可计算：P0 原子清单未封闭 |

## 阶段

| 阶段 | 状态 | 证据 / 下一步 |
| --- | --- | --- |
| 文档落地 | 已完成 | [主实施计划](../plans/reaper-parity-implementation.md)；只表示计划已写入 |
| P0 | 进行中 | [实机动作目录](../verification/reaper-parity/linux-7.82/README.md)已采集；界面/鼠标/偏好和验收预算未封闭 |
| P1 | 进行中 | [快捷键基础能力](../verification/reaper-parity/keyboard-foundation.md)；Action List Filter 与轨道名焦点的首批原厂键盘行为已核验并纳入回归；Section、完整 Action List、数字键盘/物理键和事务仍待推进 |
| P2 | 进行中 | Desktop/Touch profile 已分离；Arrange/Mixer/Media Browser 同时显示、独立分割比例及布局持久化；浮动 Docker 等仍待开发 |
| P3 | 进行中 | 普通轨道输出目标、接收路径 Solo、默认 0 dB 声像与旧工程策略迁移；并行 Post-fader Send/Receive 基础已实现；三种 Tap、嵌套文件夹/Parent Send 与压缩基础已实现；通道/MIDI 等仍待开发 |
| P4 | 进行中 | Item 手动淡化曲线及工程/DSP 基础；Item Nudge/Set 已覆盖多目标、Duplicate 与 Audio/MIDI Contents；schema 50 signed section-relative audio source offset 已跨 Core/Storage/Media/UI 接入，schema 51 MIDI source loop length 与重复事件已接入 Core/Storage/playback/timeline preview/export；looped MIDI Contents Nudge 与 Snap 开/关、mixed Audio/MIDI selection、stepped-tempo 起点及一个 linear-tempo ramp 案例已有 REAPER 7.82 对照和回归；linear source-loop 长度/相位精度、可见/可编辑 source-loop length、旧 trimmed source 长度恢复、looped MIDI 的绝对 Set→Contents（Snap off 与整秒 Snap-on）、Audio Set、mixed Audio/looped-MIDI Set 和 non-loop MIDI Set 已按原厂值实现；non-loop MIDI Contents Nudge、其他 Target 与更广的 Undo 仍待补；MIDI Item Properties 的 Loop source 开关已实现、实窗对照且可撤销，关闭时保留 MIDI source loop length；MIDI Editor 钢琴卷帘/CC/Pitch Bend 循环实例显示、命中与源事件编辑已有回归和原厂/AAADAW实窗截图；Project Marker 插入/导航/删除/编号动作/可见性/标尺选择拖动/属性窗/基础 Manager，以及 Project Region 数据、可见性、时间选择插入、Manager 属性和基础时间尺显示已接入；LoopPoints 独立状态/链接动作、Action List on/off 状态、40632/40633 Loop 边界导航、Ruler 手势、项目级范围与全局 link 偏好保存恢复已接入；播放中 play cursor、Repeat、Region 原厂交互、tempo/timebase、GUI 边界与自动 Crossfade 待推进 |
| P5 | 未开始 | 高级编辑与 Take/Comp |
| P6 | 进行中 | 工厂 Play/Stop 与 Play/Pause 已分离；Repeat、完整状态与录音/设备行为待实现/实测 |
| P7 | 未开始 | MIDI |
| P8 | 未开始 | 自动化 |
| P9 | 未开始 | 插件 |
| P10 | 进行中 | 工程帧率/时间码、Notes 与 General 起始速度/拍号/采样率/有符号 Start measure/BPM basis/标尺间隔锚点/Project start time 显示偏移/已实现字段的默认工程设置保存；媒体默认已有 64pt playback/192pt render sinc，Render 支持 Entire project/Time selection/Custom time range/All project regions/All project markers、默认关闭且整数毫秒输入的 Tail、Project/常用预设输出采样率转换、Mono/Stereo WAV、PCM8/16/24/32、Float32/64、4-bit IMA ADPCM、2-bit cADPCM 与 8-bit u-Law、可调 sample-peak、Selected tracks (stems)/pre-fader/via master 与 Master mix and stems；cADPCM 编码尚未逐字节一致、Selected regions/markers、其他 Bounds、多声道、其他归一化/限制后处理和完整原厂对照仍缺 |
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

提交 `682c199` 注册 40529 `Item: Select item under mouse cursor (leaving other items selected)` 与 40530 `Item: Toggle selection of item under mouse cursor`，均无默认快捷键。它们复用同一 Arrangement hover target：40529 永远保持目标选中并保留已有选择，40530 对目标增删选择；二者不改工程/Undo，无有效 hover target 时禁用。

`select_item_under_mouse_leaving_other_items_selected_adds_without_toggling` 覆盖两次添加仍不取消已有/目标项；`toggle_selection_of_item_under_mouse_adds_and_removes_the_hovered_item` 覆盖移除和重新加入。完整 `aaadaw` nightly+Cranelift nextest 为 512/512 passed、0 skipped；Stable all-target strict Clippy、fmt、diff 和矩阵 80 项/15 列唯一 ID/证据路径检查通过。Cranelift 显示既有 iced/wgpu recursion-depth warning；Stable Clippy 无警告。`ITEM-SELECT-007/008` 保持 `in_progress`：多选后的原厂 active item/anchor 及 Action List 焦点/hover 边界尚未探测；100% 复刻仍未完成。

### 2026-10-13 Main action 41115：Invert selection

动作目录确认 Main 41115 `Item: Invert selection` 无默认快捷键。AAADAW 注册 `item.invert-selection`，Action List Run 反转整个工程的 Audio/MIDI item 选择集合，按轨道及 item 顺序更新活动项，不改变工程修订或 Undo/Redo。没有 items 时禁用。

新增 `invert_item_selection_action_selects_the_complement_of_audio_and_midi_items` 覆盖 Action List 搜索/Run、跨轨 Audio/MIDI 混合集合、重复执行恢复原选择和工程历史不变；Stable `aaadaw` 全套及 nightly+Cranelift nextest 均为 513/513 通过，strict Clippy `-D warnings`、fmt、diff 和 parity CSV 81 项/15 列唯一 ID/证据路径检查均通过。Cranelift 保留既有 iced/wgpu `TimelinePipeline: MaybeSync` recursion-depth warning，Stable Clippy 无警告。`ITEM-SELECT-009` 保持 `in_progress`：原厂空/部分选择启用边界、活动 item 细节和实窗焦点尚未探测。

### 2026-10-13 Main actions 41127/41128：相邻非重叠 item 选择

官方 7.82 / Default 7 的真实点击探针确认 41127/41128 只在边界精确相接时选中同轨目标；有间隙时无目标。多选逐项映射，映射不到的源项会从结果移除，全部无目标时清空选择，光标不动。实现现在将 Audio sample 与 MIDI tick 转换到共同项目 tick，比对精确边界，并按原厂清除/多选行为替换旧的“允许间隙、单项锚点”假设。

`adjacent_non_overlapping_item_actions_navigate_within_the_active_track` 和 `adjacent_item_actions_require_touching_boundaries_and_map_each_selection` 覆盖精确边界、跨媒体类型、多选逐项映射、无目标清空、Action List Run 及项目/Undo 不变；原厂点击记录见 [item-navigation fixture](../verification/reaper-parity/fixtures/item-navigation-native.tsv)。`ITEM-SELECT-010/011` 保持 `in_progress`：空选择的动作可用性、Action List 键盘焦点、完整实窗布局与全部 item-selection 交互仍未验收。

### 2026-10-13 Main actions 41167/41168：移动光标到最近 item 边缘

动作目录确认 41167/41168 左右最近 item edge 导航无默认快捷键。原厂运行探针确认范围为整个工程的 Audio 与 MIDI items，即使没有 item 选择也可运行；等于当前光标的边缘会跳过。AAADAW 比较全工程 Audio sample 和 MIDI tick 起止边界，按当前 edit cursor 查找严格更左/更右的最近边缘；只更新时间线 edit cursor、保留 item selection，不修改工程历史。方向上没有边缘时禁用。

`item_edge_navigation_actions_move_to_nearest_audio_or_midi_boundary` 覆盖混合边缘、无选择、方向无目标、相等端点、选择保留及工程/Undo 不变；原厂结果见 [item-navigation fixture](../verification/reaper-parity/fixtures/item-navigation-native.tsv)。`ITEM-SELECT-012/013` 保持 `in_progress`：完整 Action List 焦点和 Arrange 实窗外观还未验收。

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

新桌面 Build ID `c1e3833d58fbf8c1c257f9db9c8b2f25111dc592`、SHA-256 `69cfa5bc5e08ab5eb613c1602675caae5fedc64f59d7599607ccf05ed2dffc36` 通过 bundled Xvfb build-and-smoke helper，渲染真实 1280×800 主窗，startup 截图与标准 fixture 像素一致；另有同一构建的 Set → Contents 3 秒实窗截图，截图没有 selected item，只用于核对窗口和控件。新 TSV/截图和哈希、当前代码/回归、open differences 已加入 [`ITEM-NUDGE-SET-001` 矩阵](parity-matrix.csv)。变量 tempo Set、含 non-loop MIDI 的混合选择、non-loop MIDI Contents Nudge、更多 Snap/target 和完整 Undo/UI parity 仍开放；`ITEM-NUDGE-SET-001`、P4 与唯一 PR #282 继续 `in_progress`。

### 2026-10-15 Non-loop MIDI Contents Nudge source boundary

Linux REAPER 7.82 / Default 7 原厂探针补齐非循环 MIDI Contents Nudge 穿过 source offset 零点的三种表示：起始 offset 100 ms 时，左移 250 ms 把 offset 增至 350 ms、事件与 10 QN source 不变；右移 250 ms 把 offset 夹到零，事件前移 288 PPQ、source 扩到 10.3 QN；从零左移 250 ms 时 offset 仍为零，事件后移 480 PPQ、source 缩到 9.5 QN。更完整的前后态、截图和探针见[非循环 MIDI Nudge 验证](../verification/reaper-parity/item-nudge-midi-nonloop-contents.md)。

AAADAW 的非 loop MIDI Contents Nudge 现在保留非负 source offset，并在需要时通过 signed source-event shift 重基准 notes、CC 与 pitch bend，扩展或缩短 MIDI source。操作仍以一个 `SetMidiItemContents` 事务提交，Undo/Redo 保留 note IDs 与 item selection。新增 app regression 对照原厂 100 ms 与零 offset 状态；Nudge 定向 24/24、AAADAW App 全套 591/591、Core/App all-target Clippy `-D warnings` 均通过。新桌面二进制由仓库 smoke helper 构建并在独立 Xvfb 实际运行，1280×800 主窗与 Nudge → Contents 250 ms 实窗均截图验证；该路径使用捆绑 REAPER runtime 的 Xvfb 和 OpenGL，适用于当前没有 DISPLAY 的会话。Build ID、SHA、截图哈希见[桌面运行记录](../verification/reaper-parity/desktop-runtime.md)。格式、diff 与 matrix 检查通过；变量 tempo、source 起始事件越界和更广 Undo 语义仍开放，`ITEM-NUDGE-SET-001`、P4 与唯一 PR #282 继续 `in_progress`。
