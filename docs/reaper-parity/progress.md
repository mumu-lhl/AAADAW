# 对齐进度

## 会话记录

| 字段 | 状态 |
| --- | --- |
| 用户批准 | 2026-10-09；Linux / REAPER 7.82 / 默认主题与配置 |
| 调研提交 | `8cff888a44b67590f0969a3ece82d4ad9e9e0dba` |
| 唯一开发分支 | `feat/reaper-parity` |
| 唯一 PR | [#282](https://github.com/mumu-lhl/AAADAW/pull/282)，Draft；后续阶段只更新此 PR |
| 当前交付 | 唯一开发分支与 Draft PR #282 持续推进；当前 P10 切片覆盖 Project Settings Notes、General BPM/拍号/采样率、有符号 Start measure、BPM basis、标尺间隔锚点、Project start time 显示偏移、受支持字段的默认工程设置保存、64-tap sinc 播放、192-tap sinc 离线渲染、Entire project/Time selection 范围、毫秒精度尾音、Project/常用预设输出采样率转换及 Mono/Stereo WAV 导出；多声道、其他 Source 和完整 Render 对照仍未完成 |
| 产品覆盖率 | 尚不可计算：P0 原子清单未封闭 |

## 阶段

| 阶段 | 状态 | 证据 / 下一步 |
| --- | --- | --- |
| 文档落地 | 已完成 | [主实施计划](../plans/reaper-parity-implementation.md)；只表示计划已写入 |
| P0 | 进行中 | [实机动作目录](../verification/reaper-parity/linux-7.82/README.md)已采集；界面/鼠标/偏好和验收预算未封闭 |
| P1 | 进行中 | [快捷键基础能力](../verification/reaper-parity/keyboard-foundation.md)；Section、完整 Action List、数字键盘/物理键和事务仍待推进 |
| P2 | 进行中 | Desktop/Touch profile 已分离；Arrange/Mixer/Media Browser 同时显示、独立分割比例及布局持久化；浮动 Docker 等仍待开发 |
| P3 | 进行中 | 普通轨道输出目标、接收路径 Solo、默认 0 dB 声像与旧工程策略迁移；并行 Post-fader Send/Receive 基础已实现；三种 Tap、嵌套文件夹/Parent Send 与压缩基础已实现；通道/MIDI 等仍待开发 |
| P4 | 进行中 | Item 手动淡化曲线及工程/DSP 基础；GUI、完整基本编辑和自动 Crossfade 待推进 |
| P5 | 未开始 | 高级编辑与 Take/Comp |
| P6 | 进行中 | 工厂 Play/Stop 与 Play/Pause 已分离；Repeat、完整状态与录音/设备行为待实现/实测 |
| P7 | 未开始 | MIDI |
| P8 | 未开始 | 自动化 |
| P9 | 未开始 | 插件 |
| P10 | 进行中 | 工程帧率/时间码、Notes 与 General 起始速度/拍号/采样率/有符号 Start measure/BPM basis/标尺间隔锚点/Project start time 显示偏移/已实现字段的默认工程设置保存；媒体默认已有 64pt playback/192pt render sinc，Render 支持 Entire project/Time selection、毫秒精度尾音、Project/常用预设输出采样率转换及 Mono/Stereo WAV，但多声道、Source 模式、其他范围与完整原厂对照仍缺 |
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
