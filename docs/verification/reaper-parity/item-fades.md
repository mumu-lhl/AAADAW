# Audio Item 手动淡化基础

参照：Linux REAPER 7.82 / Default 7 / 独立初始配置，2026-10-10。
矩阵：`ITEM-FADE-001`，仍为 `in_progress`；此记录不代表 Item Properties、自动 Crossfade 或鼠标行为已经完整对齐。

## 实机行为与逐帧证据

自有 [Lua 探针](../../../scripts/reaper_parity/probe-item-fades.lua)只在空白、未保存的独立工程中创建轨道、插入自有常量媒体并离线渲染。版本、主题与空白工程均有守卫。输入是 48 kHz / 单声道 / 16 位 / 一秒、幅度 0.125 的 WAV；输出为 48 kHz / 双声道 / 24 位 / 48,000 帧，关闭 Tail。未提交原厂资源、截图或音频。

API `AddMediaItemToTrack` 创建路径默认淡入/淡出各 0.01 秒；界面媒体插入路径 `InsertMedia` 则默认两者均为零。两条路径的默认形状均为 1。自动长度与方向均为零。不能把 API 创建默认长度错误应用到每次文件导入。

归一化位置 `x` 从零到一，七种兼容形状（`C_FADEINSHAPE` / `C_FADEOUTSHAPE`）的淡入曲线实测为：

| 原厂编号 | 曲线 |
| --- | --- |
| 0 | `x` |
| 1（默认） | `x * (2 - x)` |
| 2 | `x²` |
| 3 | `1 - (1 - x)⁴` |
| 4 | `x⁴` |
| 5 | `3x² - 2x³` |
| 6 | 前半段 `8x⁴`；后半段 `1 - 8(1 - x)⁴` |

淡出使用剩余时长归一化；一秒 Item 的最后一帧仍在 Item 内，线性 0.25 秒淡出该帧的增益为 `1 / 12000`，Item 的排他结束点为零。

请求长度保留原值，渲染有效长度满足：`in = min(requested_in, Item_length)`；`out = min(requested_out, Item_length - in)`。淡入优先。0.75/0.75 秒请求在一秒 Item 上实际成为 0.75/0.25 秒；1/1 或 2/0 秒请求均为整段一秒淡入。两条重叠手动曲线不会相乘。通过 `UpdateItemInProject` 刷新后重复渲染，规则不变。

[逐帧校验脚本](../../../scripts/reaper_parity/verify-item-fades.py)检查七种曲线及五组超长/重叠案例的全部 1,152,000 个声道采样点；归一化增益最大误差小于 `4.768e-7`，小于一个 24 位 PCM 量化步长。每个输出的散列和误差见 [事实记录](item-fade-reference.json)。常量幅度归一化，因此此处不是主观听感或少数截图比较。

复验时为每次探针创建独立配置与输出目录，生成上述自有输入 `source.wav`，设置 `AAADAW_REAPER_PROBE_DIR`，用 REAPER 的 `-cfgfile`、`-newinst`、`-nosplash` 启动该 Lua；随后运行 `python3 scripts/reaper_parity/verify-item-fades.py <输出目录>`。原厂可执行文件及输出只存于工作区 scratch。

## 实现边界

`AudioFade` 验证有限非负的小数采样长度，`FadeShape` 保持原厂 0–6 编号；`AudioItemFades` 计算 Item 相对时钟上的增益。新导入和旧工程默认零长度、形状 1。请求超长长度保留，修改 Item 长度时重新计算有效淡化。

公开 `Project` / `SetAudioItemFades` 支持两端参数一次撤销；移动、修剪、快照和完整音频 Item 复制保留参数。普通复制和 Ctrl 拖动复制使用核心完整复制动作，不预测新 Item ID。

schema 25 新增 `audio_item_fades` 表；旧 Item 没有行时恢复零长度。写入在工程事务内，删除 Item 时同步清理。加载验证形状、有限非负长度与孤立行；只读加载不修改原文件。

真实 PCM 在 source 相加到轨道 FX 输入前应用一次淡化，时钟不受媒体 source offset 影响；处理涵盖 Item 起止、跨 callback 与中途 seek，不改变未放置 Item 的传统连续轨道源及输入监听路径。

尚待：GUI 淡化手柄及曲线菜单、原厂手势/修饰键、自动 Crossfade、自动分割淡化、Take/Item gain 和 Item Properties 全界面。基础模型不宣称这些能力已经完成。

## 本地验证

默认首轮 `cargo xtest --no-fail-fast`：794 项全部通过；随后包括新增集成测试的 `cargo xtest --features aaadaw/audio-device --no-fail-fast`：819 项全部通过、零跳过。默认 `cargo clippy --workspace --all-targets -- -D warnings` 通过。音频构建仍有既存 unused_mut/dead_code 两项警告，未宣称该 feature 下 warnings denied 通过。

回归覆盖真实双声道 PCM、七种曲线、非零 Item 起点及 source offset、跨 13 帧 callback、淡出区域 seek、小数/超长长度、Undo/Redo、完整复制、Snapshot、批处理失败原子性、schema 24 默认迁移、只读重开字节不变与非法长度拒绝。默认已有无分配 callback 回归也通过；本切片没有对 GUI 淡化手柄作未执行的验收。


补充原厂 UI 观察：选中单个音频 Item、F2 打开 Media Item Properties，客户端 526×693。该窗口同时包含 Fade in/out 时间、两组 Curve 字段和曲线按钮；超长淡出仍显示 0:02.000，而 Item Length 显示 0:01.000，与保存请求长度的 API 行为一致。完整属性字段和批量编辑将另行实现和验收，未用简化 Inspector 代替。

## 实时发布切片

ItemFadeController 按固定图中的 ItemId 发布完整淡入/淡出参数对。控制侧写入串行化；callback 对原子序号和参数只尝试读取一次。若写入中断或两次序号不同，保持上一组完整值，无自旋、锁或分配。图替换时更新控制句柄，暂停期间的参数会在下一次实际源渲染读到。

SetAudioItemFades 使用独立的参数事件，允许播放中的单项 Undo/Redo；事件只改淡化，不覆盖 Item 位置、媒体或其他参数。App 同步普通提交与含淡化的批事务；历史操作同步当前所有淡化。批事务整体历史在播放期间仍沿用现有非实时历史守卫，未将所有混合结构编辑错误归类为实时安全。

21 项相关回归通过；最终 audio-device 全 workspace 822 项通过、零跳过，默认 Clippy warnings denied 通过。新增真实 PCM 更新/Undo/Redo、固定图中未知 Item 拒绝、控制写入中途停顿和并发完整参数对校验。回调分配检测新增非默认小数长度的实时发布，仍为零分配。GUI 手柄与预览状态仍未接入，原厂自动 Crossfade、方向和批量鼠标修饰键仍待推进。


## REAPER 7.81+ 连续曲率与 S 参数

[官方 ReaScript API](https://www.reaper.fm/sdk/reascript/reascripthelp.html#GetMediaItemInfo_Value)说明 `D_FADEINDIR_NEW` / `D_FADEOUTDIR_NEW` 和 `D_FADEINDIR2_NEW` / `D_FADEOUTDIR2_NEW` 是 7.81 后的连续曲率/S 参数，均为 −1..1。旧 C_FADE* 编号不能独立表达全部 7.82 曲线。

补充自有渲染探针测量 13 个连续/混合参数案例；结合前 12 个案例，逐帧核对共 25 个双声道、48,000 帧输出，即 2,400,000 个声道采样点。全部误差不超过 `4.768372e-7`，小于一个 24 位 PCM 量化步长；见 [散列与误差](item-fade-curvature-reference.json)。这证明记录的输入网格与案例，未把有限案例称为任意输入的全量 UI 验收。

测得曲率先在 `x` 上作变换，再应用 S 变换。定义 `P(x,a)`：`a<=0.5` 时线性插值 `x` 与 `x²`，权重 `2a`；否则插值 `x²` 与 `x⁴`，权重 `2a-1`。负曲率取 `P(x,abs(c))`，非负取 `1-P(1-x,c)`。正 S 在半段分别计算 `0.5*P(2x,s)` 与 `1-0.5*P(2(1-x),s)`；负 S 使用对应反向半段。组合 c=0.25 / s=0.5 的逐帧数据确认计算顺序。

不能仅存两个曲率值后丢掉兼容模式：旧 Smooth 的 getter 也返回 c=0 / s=0.5，但其在 x=0.25 的增益是 0.15625；显式设置新 S=0.5 后增益为 0.125。自有 [参数探针](../../../scripts/reaper_parity/probe-item-fade-parameters.lua)和 [状态事实](item-fade-parameter-reference.txt)显示，Item 的公开 state chunk 分别保留兼容标志和 `FADE_NEW_PARAMETERS`，与真实输出差异相符。未通过相同 getter 值臆测两种模式等价。

领域新增 `FadeCurve::Legacy` / `Native` 和验证过的 `FadeCurveParameters`；默认与 schema 25 保留已有兼容曲线，显式新曲线保留完整模式及两个参数。schema 26 的 `audio_item_fade_curves` 只存非兼容曲线的可空参数对；旧行不覆盖。加载拒绝不完整、越界、非有限参数及孤立行，事务写入和删除包含曲线表。实时控制将模式和四个曲率数值纳入同一个有界快照，避免模式与参数混读。

最终 audio-device 全 workspace 825 项通过、零跳过；默认 Clippy warnings denied 通过。24 项相关回归通过；测量常量明确为 f64 后，5 项曲线集成回归再次通过。回归包括连续曲率的独立 PCM 点、81 组参数的边界/单调性、兼容/新模式的实际输出差异、混合模式保存与 schema 25 保留、非法参数对拒绝、同图 live 更新、跨模式并发发布和非默认曲率下零回调分配。

迁移测试最初误把只读打开视为迁移入口，已修正测试：旧 schema 只读拒绝且字节不变，可写入口升级后保留原曲线。未改动产品的只读约定。GUI 手柄和连续曲率编辑尚未接入。

## 默认淡化偏好与鼠标行为观察

实机 Preferences → Project → Item Fade Defaults：默认 Fade in/out 和 Crossfade 均为 0:00.010；Imported media items 的 Fade in/out **未勾选**，Recorded media items 与 Split media items **勾选**，各自 Crossfade 为 No crossfade；限制 split 淡化到 50 pixels 未勾选，对 MIDI 应用淡化偏好未勾选。相同默认长度不能用于所有创建路径；录音和分割的自动淡化尚待实施。

Preferences → Editing Behavior → Mouse Modifiers，Context=Media item fade/autocrossfade / left drag：默认 Move fade ignoring snap；Shift 为 Move crossfade ignoring snap；Ctrl 还忽略 selection/grouping；Alt 涉及 stretch crossfaded items；Ctrl+Alt 为 relative edge edit。单个无 Crossfade Item 的普通/Shift 小幅拖动均保留非网格值，与默认绑定相符；不把这些例子当作选中集合/交叉淡化/拉伸/相对边缘全部行为通过。

[鼠标探针](../../../scripts/reaper_parity/probe-item-fade-interaction.lua)只在空白独立工程中插入自有媒体，逐帧记录屏幕指针、命中 Item、长度与兼容形状。初次自动化使用的指针定位/warp 事件未改变淡化；改用 Linux XTestFakeMotionEvent 后，实际参数连续更新。例如 130 像素位移生成 0.482912332838 秒，随后 6 像素位移生成 0.505200594354 秒，Ctrl+Z 恢复前值。故不以先前工具事件失败认定产品手柄有 bug，也不硬套 Trim 的默认吸附行为。参照截图只保存在 scratch。

## Timeline manual fade handles

The current slice adds handles at the effective fade endpoints in the audio body, a curve drawn from the same domain curve evaluation as PCM, and live parameter previews. Plain dragging ignores grid snap, as observed in the Linux 7.82 default Mouse Modifiers preferences. Moving below the handle band retains ordinary item editing. Frozen render items are not editable through these handles.

A drag records a window pointer anchor, so selecting the item and resizing the inspector cannot alter the length calculation. Floating tick/sample conversions reuse the tempo-map integration and inverse, including Step/Linear/Log/Bezier tempo segments; they do not quantize the gesture to integral ticks or samples. Preview retains the original Legacy/Native curve mode. Mouse release commits one core action; Escape/right button restore the saved parameters without a history entry. Unrelated domain actions, Undo/Redo, and project transitions cancel a pending preview. Busy import/I/O/preparation guards apply; active playback uses the bounded item-fade controller.

Initial Canvas stroke overlays left old curve fragments during this virtual-display experiment. The final implementation draws bounded visible curve segments and handle outlines in the timeline GPU primitive alongside item/background/waveform rendering. Final AAADAW-only [manual-fade](item-fade-ui/manual-fades.png), [Undo](item-fade-ui/undo-fade-out.png), and [Redo](item-fade-ui/redo-fade-out.png) captures use an owned mono constant source. Pixel inspection found 99 fade-out red pixels in the defined right edge region before Undo and zero after Undo; both captures contained zero red pixels in the adjacent TCP region. This proves the observed residue regression for this display, not full theme or DPI acceptance.

Window test: at 62.399998 pixels/quarter, a 20-pixel drag changed fade-in from 36538.46287868437 to 28846.1549042245 samples. A 20-pixel fade-out drag saved 7692.307974459873 samples. Right-button cancellation retained the previous fade-in. Escape during fade-out preview followed by Undo removed the preceding committed fade-out; Redo restored the exact saved length and original shape 1. Saved scratch schema 26 rows were read only after the deferred session snapshot completed, not immediately after mouse release. Recovery reopened the owned item and its curve.

Final `cargo xtest --features aaadaw/audio-device --no-fail-fast`: 828 passed, zero skipped. The intervening display-service exit caused two CLAP GUI test failures; after restoring the owned displays the full final suite passed. Default `cargo clippy --workspace --all-targets -- -D warnings` passed. Focused gesture/history/fractional-clock regressions also passed, including changing the timeline bounds while the pointer drag is active.

Remaining: numeric fade inputs, current-version curve editor/menu, selected/grouped-item behavior, Shift crossfade and other modifiers, native drag-overlap adjustment, moving/trimming curve previews, automatic recording/split fades, auto/manual Crossfade, complete Item Properties, and visual comparison across sizes/DPI. This slice does not close EDIT-FADE-001, ITEM-FADE-001, P4, or whole-product parity. Physical playback monitoring is unavailable here; controller/PCM proof is covered by the earlier audio tests.

## 七种曲线菜单（Linux 7.82 实测）

右键单个音频 Item 的淡化手柄打开七种预设，淡入图位于 96×24 图框的左半段，淡出图位于右半段。行间距 28、菜单宽 152，以及灰色背景、曲线填充和悬停颜色来自实机测量；数值和自有工程的 FADE 状态事实见 [菜单参照](item-fade-menu-reference.json)。原厂截图仅留在 scratch，未纳入仓库。

原厂选择 0–4 和 6 保留兼容模式；选择 5（Smooth）写入 FADE_NEW_PARAMETERS，使用 c=0、s=0.5。旧 Smooth 即使显示同一选中项，再选择仍须转换模式，不能因为编号相同跳过。当前菜单经公开 DawAction 提交、只改变指定一端曲线；重复选择同一实际模式不增加历史。

AAADAW 实窗 [淡入菜单](item-fade-ui/curve-menu-in.png)和 [淡出菜单](item-fade-ui/curve-menu-out.png)均可选择。淡入 Smooth 保存后两端长度仍为 28846.1549042245 / 7692.307974459873 samples，曲线表为 (0, 0.5) / NULL；Undo 恢复 shape 1 并删除 Native 行，Redo 恢复。随后将淡出改为 Linear，仅修改其兼容编号为 0，保留淡入 Native 参数及两端长度。Escape 和点击菜单外时间线关闭菜单；后一点击仍执行普通编辑光标定位。

最终 audio-device 全 workspace **830 passed / 0 skipped**；默认 Clippy warnings denied 通过。新增回归覆盖全部预设模式、旧/新 Smooth 输出差异以及曲线选择的历史、重复选择、长度与另一端参数保护。上一手柄提交 d2bfbe2 的 Rust CI、Portable desktop archives、Native installers 全部成功。

仍待完成：原厂菜单末尾的 Low pass fade（I_FADELPF 淡入/淡出位标志独立）、曲率数值/连续编辑、菜单方向键导航、抗锯齿/DPI 精确验收、多 Item/分组修饰键和 Crossfade。当前七行菜单高度 208，原厂含低通入口为 224；这项差异保留为未完成，不宣称整个菜单对齐。

## Item Properties 淡化数值编辑基础

独立 Media Item Properties 窗口接入 Item 菜单和 F2，区分“打开”（原厂动作 40009）与“切换”（41589，默认 F2）。在主窗口或属性窗口获得焦点时，F2 都可切换关闭；用户显式绑定优先。当前仅单个 Audio Item 的淡化字段，窗口 526×210；原厂完整 526×693 属性窗口包含其他 Item/Take/Source 能力，尚未完成其字段及几何。

输入草稿与工程分离，Apply/OK 经 SetAudioItemFades 一次提交；Cancel/Escape 不改工程。未编辑字段保留完整小数长度及 Legacy/Native 模式，编辑曲率保留同端 S 和另一端参数。非法/非有限输入留在草稿并显示错误；曲率有限值按实测限制至 −1..1。旧工程窗口草稿不能应用到另一工程 generation。

[原厂事实](item-fade-properties-reference.json)确认：属性窗口的长度输入与 API 请求长度不同，编辑长度会限制到 Item 长度并缩短另一端以避免重叠。只改淡入时淡入优先；两端都改时 Apply 按淡入、淡出顺序处理，淡出优先，与输入顺序无关。底层领域继续保留 API 请求长度，不把界面归一化规则扩散到存储迁移。

实窗将淡入曲率从 0 改为 0.25，输入阶段保存行仍为 (0,0.5)，Apply 后为 (0.25,0.5)；两端长度仍为 28846.1549042245 / 7692.307974459873 samples，淡出仍为 Legacy Linear。随后输入 0.9 并 Escape，Undo 恢复上次已应用的曲率 0，证明取消草稿未添加历史；Redo 恢复 0.25。将淡出输入 0:02.000 并 Apply，实窗与保存行均归一为淡入 0 / 淡出 48000 samples，曲线模式仍保留。最新构建恢复会话后再次实测 F2 开关，属性窗口焦点下第二次 F2 关闭成功。

[当前属性窗口](item-fade-ui/properties-fades.png)为 AAADAW 自有截图；[超长输入归一化](item-fade-ui/properties-normalized.png)记录支持的长度行为。原厂截图仍仅在 scratch。最终 audio-device 全 workspace **833 passed / 0 skipped**；默认 Clippy warnings denied 通过。新增回归包括草稿提交/取消、Undo/Redo、模式/精度保护、双端重叠处理、非法输入、工程 generation 防护，以及 F2 和自定义绑定。曲线菜单提交 7ab788a 的三项远端流程全部成功。

仍待：完整 Item/Take/Source 属性、位置/长度显示单位、批量编辑与 mixed values、曲线按钮/连续鼠标编辑、完整主题与焦点细节、低通、自动淡化和 Crossfade。当前淡化字段切片不关闭 UI-009、P4 或全量对齐。

## 属性位置、长度与源起点

属性草稿新增 Position、Length、Start in source 的 Time 输入，当前使用既有非负整数采样领域时钟。位置/长度/源起点与淡化修改先全部验证，再以单动作或 BatchTransaction 一次提交、一次 Undo；结构修改在播放中受保护，纯淡化继续走实时发布。非法、零长度或超出可精确表示范围的输入保留错误草稿且不改工程。

原厂实测只缩短 Item Length（1→0.25 秒）时，淡化请求 0.8/0.2 秒保持原值；本实现也仅在直接编辑淡化长度时归一化。位置修改和源偏移不改变淡化时钟或模式。长度与淡化同时编辑时，归一化使用此次拟提交的新 Item 长度，而非旧长度。

最新构建实窗将 Position/Length/Start in source 一起设为 0:00.500 / 0:00.250 / 0:00.125；保存行分别为 start=24000、length=12000、source_offset=6000，淡化请求仍为 28846.1549042245 / 7692.307974459873 samples，Native 曲率/S 0.25/0.5 与另一端 Legacy Linear 保持。一次 Undo 的 [属性窗口](item-fade-ui/properties-placement-undo.png)恢复 0/48000/0，之后延迟 session snapshot 行也恢复；[Redo](item-fade-ui/properties-placement-redo.png)恢复三个值。即时读取自动保存文件可能尚未落盘，因此以当前窗口和完成防抖后的行交叉验证。

最终 audio-device 全 workspace **834 passed / 0 skipped**，默认 Clippy warnings denied 通过。新增事务回归包含位置/长度/源起点与曲率同时提交及一次 Undo/Redo、缩短时保留超长淡化请求、无效长度/超范围输入的原子拒绝，以及用新长度处理淡化重叠。

仍待：原厂 Time/Beats/H:M:S:F/Samples 单位切换、负位置和分数采样 Item/源位置、Loop source、Take 等完整字段和批量编辑；当前窗口 526×310 的字段切片不视为完整 526×693 原厂窗口验收。自有 [参照事实](item-fade-properties-reference.json)补记单位选择：Samples 会同时改变 Position/Length/Start in source，淡化时间仍为 Time。

## Time / Samples 显示与单位切换

属性窗口接入 Time / Samples，默认 Time；Samples 同时改变 Position、Length 和 Start in source，淡化时间仍显示 Time。单位在独立版本化配置 item-properties.json 中原子保存，关闭/重开及重启恢复；未知版本或非法值加载失败且不改原文件。工程 schema 仍为 26。

进一步实测修正初版假设：原厂切换单位会从当前工程刷新**全部字段**，丢弃尚未 Apply 的位置和曲率草稿，并禁用 Apply。当前实现按此行为处理，不保留未应用草稿。已应用的数值不被显示格式改写：Native Samples 位置 16001 经 Apply 后为 0.333354166667 秒，切回 Time 仅显示 0:00.333；24035 显示 0:00.500。

自有 [时间格式探针](../../../scripts/reaper_parity/probe-time-displays.lua)在独立空白工程读取当前帧率并控制 48 kHz、Tempo/Meter 案例，产生 [27 组、108 行数值事实](time-displays-reference.tsv)。Time 在 10 ns 级边界容差后截取完整毫秒，不使用三位小数四舍五入；1 小时后显示 H:MM:SS.mmm。当前 Time 回归逐项核对 27 组 position/length 共 54 个输出，包含纳秒边界、分钟/小时、手动小数淡化和精确采样位置。Beats 和 H:M:S:F 数据作为后续实现参照，未将采集本身记为 GUI 通过。

最新构建实窗中，将 Samples 位置改为 20000、淡入曲率改为 0.9 后切回 Time，恢复已应用的位置 16001（显示 0:00.333）和曲率 0.25，保存行保持 (16001,12000,6000) 及 Native (0.25,0.5)。[刷新后窗口](item-fade-ui/properties-units-refresh.png)中 Apply 禁用。随后选 Samples、重启并恢复会话，[重启窗口](item-fade-ui/properties-units-restart.png)恢复 Samples 和三个精确整数值；淡入显示 0:00.600 而非先前的 0:00.601，原请求仍为 28846.1549042245 samples。

最终 audio-device 全 workspace **837 passed / 0 skipped**，默认 Clippy warnings denied 通过。回归覆盖单位刷新/草稿丢弃、未编辑采样精度、非法输入、配置读写/未来版本保护和原厂时间格式事实。前一位置提交 832858b 的 Rust CI、Portable desktop archives、Native installers 全部成功。

当前仅实现 Time / Samples 两个选项；原厂还包含 Beats / H:M:S:F，Source 在这两个模式下仍使用 Time，默认帧率实测 30 fps non-drop。完整单位转换、工程帧率、负/分数采样位置、完整属性和原厂布局继续推进；不关闭 ITEM-PROPERTIES-001 或 P4。

## Beats 参照扩展（实现验证中）

在固定 Linux REAPER 7.82 / Default 7 的空白自有临时工程中扩展 `probe-time-displays.lua`，保存 [输入数值](beat-inputs-reference.tsv) 和 [跨速度/拍号数值](beat-changes-reference.tsv)。来源仅为 API 返回的数字/文本，不包含 REAPER 二进制或主题资源。

位置的起点为 `1.1.00`，长度的起点为 `0.0.00`。Beats 下 Take 的 Start in source 仍用 Time。拍内显示按百分之一拍舍入；6/8 中一拍为八分音符。位置和长度的输入不能共享同一个起点规则。

跨拍号时显示与解析不是互逆：从 0 秒起跨 4/4→3/4 的 4 秒长度显示 `2.1.00`，重新解析得到 4.5 秒。显示使用起终点小节/拍之差，并按起点拍号借位；长度解析使用起点拍号把小节换为拍，再沿速度图换算。这些是固定版本实测行为。未编辑字段必须保留原始样本值，不得通过显示字符串重新解析。

当前先实现非负数字的标准输入；原厂负数、无效文本及尾随文字的宽松解析已采集但尚未验收。帧格式、完整负/分数采样 Item 时钟、全属性布局和其他字段仍未完成。此段不表示 GUI 或全量验收通过。


实际 AAADAW 窗口验证：初始 16001/12000/6000 样本显示 `1.1.67` / `0.0.50` / `0:00.125`（[选择 Beats](item-fade-ui/properties-beats-selected.png)）；一次 Apply 写入 24000/24000/12000 样本及 S=0.75（[Apply](item-fade-ui/properties-beats-applied.png)）。一次 Undo 恢复全部位置/长度/源偏移及 S=0.50（[Undo](item-fade-ui/properties-beats-undo.png)），Redo 恢复全部应用值（[Redo](item-fade-ui/properties-beats-redo.png)）；配置写入 `time_unit=beats`，重启恢复工程后保持 Beats 和已应用值（[重启](item-fade-ui/properties-beats-restart.png)）。截图仅含 AAADAW 自有窗口。尚未补齐原厂完整属性窗口几何。

切换单位丢弃未应用 Beats 位置和 S 草稿，恢复已应用值并禁用 Apply（[刷新](item-fade-ui/properties-beats-refresh.png)）。最终 `cargo xtest --features aaadaw/audio-device --no-fail-fast`：842 passed / 0 skipped；默认 workspace Clippy warnings denied、fmt 与 diff 检查通过。
