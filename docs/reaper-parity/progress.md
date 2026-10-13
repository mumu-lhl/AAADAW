# 对齐进度

## 会话记录

| 字段 | 状态 |
| --- | --- |
| 用户批准 | 2026-10-09；Linux / REAPER 7.82 / 默认主题与配置 |
| 调研提交 | `8cff888a44b67590f0969a3ece82d4ad9e9e0dba` |
| 唯一开发分支 | `feat/reaper-parity` |
| 唯一 PR | [#282](https://github.com/mumu-lhl/AAADAW/pull/282)，Draft；后续阶段只更新此 PR |
| 当前交付 | 唯一开发分支与 Draft PR #282 持续推进；当前 P10 切片覆盖 Project Settings Notes、General BPM/拍号/采样率、有符号 Start measure、BPM basis、标尺间隔锚点、Project start time 显示偏移、受支持字段的默认工程设置保存、64-tap sinc 播放、192-tap sinc 离线渲染、Entire project/Time selection 范围、毫秒精度尾音、Project/常用预设输出采样率转换、Mono/Stereo WAV、PCM8/16/24/32、Float32/64、4-bit IMA ADPCM、2-bit cADPCM 与 8-bit u-Law、可选 sample-peak 归一化（−60 至 0 dBFS）、Selected tracks (stems)、pre-fader stems、Selected tracks via master 与 Master mix and stems；另有 Project Marker 稳定身份/可见编号、schema36 保存/迁移、插入/导航/删除/快捷动作/拖动/属性编辑；Loop Ruler 已补充 Shift 整段移动/Snap、底栏建点、项目级 Loop/Time Selection 保存恢复、全局 link 偏好持久化、Action List link on/off 状态与 40632/40633 范围导航；cADPCM 编码、Repeat 播放、其他 P4/P6 和完整原厂验收仍未完成 |
| 产品覆盖率 | 尚不可计算：P0 原子清单未封闭 |

## 阶段

| 阶段 | 状态 | 证据 / 下一步 |
| --- | --- | --- |
| 文档落地 | 已完成 | [主实施计划](../plans/reaper-parity-implementation.md)；只表示计划已写入 |
| P0 | 进行中 | [实机动作目录](../verification/reaper-parity/linux-7.82/README.md)已采集；界面/鼠标/偏好和验收预算未封闭 |
| P1 | 进行中 | [快捷键基础能力](../verification/reaper-parity/keyboard-foundation.md)；Section、完整 Action List、数字键盘/物理键和事务仍待推进 |
| P2 | 进行中 | Desktop/Touch profile 已分离；Arrange/Mixer/Media Browser 同时显示、独立分割比例及布局持久化；浮动 Docker 等仍待开发 |
| P3 | 进行中 | 普通轨道输出目标、接收路径 Solo、默认 0 dB 声像与旧工程策略迁移；并行 Post-fader Send/Receive 基础已实现；三种 Tap、嵌套文件夹/Parent Send 与压缩基础已实现；通道/MIDI 等仍待开发 |
| P4 | 进行中 | Item 手动淡化曲线及工程/DSP 基础；Project Marker 插入/导航/删除/编号动作/标尺选择拖动/属性窗/基础 Manager，以及 Project Region 数据、时间选择插入、Manager 属性和基础时间尺显示已接入；LoopPoints 独立状态/链接动作、Action List on/off 状态、40632/40633 Loop 边界导航、Ruler 手势、项目级范围与全局 link 偏好保存恢复已接入；播放中 play cursor、Repeat、Region 原厂交互、tempo/timebase、GUI 边界与自动 Crossfade 待推进 |
| P5 | 未开始 | 高级编辑与 Take/Comp |
| P6 | 进行中 | 工厂 Play/Stop 与 Play/Pause 已分离；Repeat、完整状态与录音/设备行为待实现/实测 |
| P7 | 未开始 | MIDI |
| P8 | 未开始 | 自动化 |
| P9 | 未开始 | 插件 |
| P10 | 进行中 | 工程帧率/时间码、Notes 与 General 起始速度/拍号/采样率/有符号 Start measure/BPM basis/标尺间隔锚点/Project start time 显示偏移/已实现字段的默认工程设置保存；媒体默认已有 64pt playback/192pt render sinc，Render 支持 Entire project/Time selection、毫秒精度尾音、Project/常用预设输出采样率转换、Mono/Stereo WAV、PCM8/16/24/32、Float32/64、4-bit IMA ADPCM、2-bit cADPCM 与 8-bit u-Law、可调 sample-peak、Selected tracks (stems)/pre-fader/via master 与 Master mix and stems；cADPCM 编码尚未逐字节一致、其他范围、多声道、其他归一化/限制后处理和完整原厂对照仍缺 |
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

动作目录确认 41127 `Item: Select next adjacent non-overlapping item` 与 41128 `Item: Select previous adjacent non-overlapping item` 无默认快捷键。AAADAW 按活动选中 item 的 Audio sample/MIDI tick 边界，只在同一轨道查找最近的非重叠目标并替换 item 选择；没有目标时 Action List 禁用。实现允许目标之间有空隙，未据此声称与原厂边界已一致。

新增 `adjacent_non_overlapping_item_actions_navigate_within_the_active_track` 覆盖 Audio/MIDI 混排、重叠目标跳过、其它轨道隔离、前后边界禁用和工程历史不变。专项 nightly+Cranelift nextest 1/1、应用全套 514/514 passed / 0 skipped；Stable strict Clippy `-D warnings`、fmt、diff 和 parity CSV 83 项/15 列唯一 ID/证据路径检查均通过。Cranelift 保留既有 iced/wgpu `TimelinePipeline: MaybeSync` recursion-depth warning。`ITEM-SELECT-010/011` 保持 `in_progress`：原厂是否越过间隙、多个 selected item 的 active/anchor 规则及焦点行为尚待探测。
