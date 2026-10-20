# 领域、引擎与持久化变更

## 当前边界

基线提交 `8cff888` 的 SQLite schema 为 17。`aaadaw-core` 是唯一工程修改入口；`aaadaw-app` 编排媒体、编辑意图与后台任务；`aaadaw-engine` 执行实时处理；GUI 和存储均不得直接改领域字段。

下面是实施方向，具体类型和接口在所属切片设计时确定。不能为了贴近 REAPER 的内部名称更换现有架构，也不要求 `.aaadaw` 二进制等同 `.rpp`。

## 模型变化与责任

| 能力 | 领域不变量 | 主要落点 |
| --- | --- | --- |
| 通用轨道与文件夹 | 稳定 TrackId、有效父子树、明确父发送与折叠展示 | core `track.rs/project.rs/action.rs/snapshot.rs`；GUI TCP/MCP |
| Send/Receive 与通道 | 路由为一份有 ID 的连接；Receive 是同一连接的反向视图；Tap/通道/MIDI 映射有效 | core；app 图准备；engine 实时图；Routing UI |
| Take/Source/Item | Item placement 与 Source 内容分开；TakeId/SourceId 稳定；源非破坏性 | core `audio.rs/midi.rs`；app `audio_editing.rs/midi_editing.rs` |
| 淡化/增益/Loop/Rate/Pitch | 有效范围、明确源/工程时间、实时与离线语义一致 | core；media 处理；app playback/render；Timeline |
| Lane/Comp/Razor/Group | 引用有效、跨对象编辑原子；Comp 输出可还原 | core；app 编辑；Timeline；recording |
| Marker/Region/Tempo | 稳定 ID、单位与锚定明确；时基转换和边界校验 | core `timebase.rs`；管理窗口/标尺 |
| Envelope/Automation Item | 参数身份稳定，曲线和池化一致；显示与激活/Arm 分离 | core；app 控制；engine 自动化；GUI Lane |
| MIDI 事件与共享源 | Channel/Bus/事件类型、顺序、长度和 Pool 引用有效 | core `midi.rs`；app；engine `midi.rs`；多编辑器 |
| 插件实例与挂载点 | 实例 ID 与链序无关；格式/状态/参数/延迟可恢复 | core Track/Take/FX；app plugin owners；engine hosts/helpers |
| 录音与输入 | 输入选择/监听/模式/Take 归属明确，录音时钟不依赖 UI 帧 | core；app `live_recording.rs/capture_timeline.rs`；engine capture |
| 导出与分析 | 输入快照、范围、矩阵、命名、编码与任务取消明确 | app `offline_render.rs/wav_export.rs`；GUI Render/Queue |
| 视频/脚本/控制器 | 时间线/参数绑定与 Action 入口一致；后台或服务生命周期明确 | 按切片引入 adapter；不在回调执行脚本或网络 I/O |

旧 Bus 的专用类型与单输出路由需过渡为通用轨道，迁移后保留 TrackId、音量/声像、FX、Mute/Solo 和输出音频。不得将旧 Bus 转换为会改变原播放路径的文件夹。

当前 Instrument 与 FX 分开保存；对齐统一链时必须保留处理顺序、状态、参数、automation target 和冻结源。FX Lane 不应因链重排改为控制另一实例。

## 时间与精度

继续保留 sample-clock/PPQ 双时基。每种对象明确锚定单位、Tempo 改变策略、源偏移、Length、Rate 和负显示偏移。改变显示单位不能改变数据；对边界、Tempo ramp、长工程和池化做属性与往返测试。

源时钟、项目时钟、设备时钟与插件块时钟分开。Loop/Punch/Automation 在块内边界切分；MIDI chase/reset 和 PDC 不依赖 GUI Timer。时伸缩算法差异记录在对齐项，不隐瞒为浮点误差。

## 图与实时安全

- 控制线程验证/构建不可变图、参数计划和所需内存；回调仅使用有界已准备数据。
- 图更新通过已有安全切换/退役机制，插件、缓冲和 feeder 在回调外回收。
- 明确多通道混音、Tap Point、Parent/Master、Input/Monitor/Take FX 的处理顺序。
- PDC 从插件/硬件路径延迟计算，使用预分配 Delay；反馈路径按参照启用并明确其补偿边界。
- Decode/SRC/Stretch/Analyze/Encode/Plugin Scan/Script/Network I/O 在后台或隔离进程执行；不引入回调锁、分配或 I/O。

## 工程、配置与视图状态

| 状态 | 所有权与保存方式 |
| --- | --- |
| 音频/MIDI、轨道、路由、Take、自动化、工程设置 | Project + DawAction + SQLite Snapshot；可撤销与可迁移 |
| 嵌入媒体、解码元数据、冻结产物 | 现有资产管理；引用与任务完成原子一致，缓存可重建 |
| 设备选择、插件路径、键鼠映射、主题、工具栏、服务配置 | 本机偏好；不得把用户机器路径静默写成工程依赖 |
| Dock、窗口、Screensets、各工程视口 | 根据参照区分本机布局/工程视图；无效屏幕安全回退 |
| 播放图、Meter、临时选择、手势预览、任务进度 | 运行态；不因视图恢复覆盖持久工程 |

## 迁移与回滚步骤

1. 每切片列出数据字段、默认值、引用规则和旧版本 fixture；先证明旧工程加载得到预期语义。
2. 从实际 `CURRENT_SCHEMA_VERSION` 增量编号，更新 `user_version` 和表结构在同一事务内完成。不得预占全部未来版本或覆盖未知未来版本。
3. 对需要有损或结构改变的迁移先创建安全备份/迁移副本；WAL 按现有锁、checkpoint 与关闭约定处理，不复制不一致数据库。
4. 迁移旧 Bus/FX/Item/配置时保留稳定 ID 和原媒体，校验引用、冻结源及 plugin state。失败回滚事务，旧数据保持可用。
5. 测试 v17 到新版本、已有历史 fixtures 顺序迁移、新版本往返、缺媒体/插件、空间不足、崩溃和未来版本拒绝。
6. 回滚提供恢复未迁移备份的路径；旧程序拒绝无法读取的新 schema。不得宣称数据库降级能够无损保留旧程序不认识的数据。

Undo/Redo 为运行会话历史；恢复历史与重开工程是否持久化按参照另行定义，不让 SQLite migration 伪造编辑历史。批量 Action 失败、保存失败和撤销失败都必须有稳定可理解结果。

## 切片设计模板

每个实现任务附：矩阵 ID、前后可观察行为、涉及模块、领域接口、存储迁移、引擎处理顺序、线程/内存所有权、窗口生命周期、错误/取消路径、验证场景、已知依赖和回滚方式。无实际需求不新增 crate 或抽象层。

## 已实现 schema 18 切片

`project_meta.pan_mode` 为受 CHECK 约束的工程级策略：0 为旧 AAADAW mono/stereo 混合策略，1 为新工程默认 0 dB balance。v17 加列默认 0，不改原轨道与媒体；新工程写 1。ProjectSettings 和 Track 构建缓存一致，Snapshot 保留 Settings；初始图与 live mix 都使用同一策略。没有 FX 前的声像补偿。并行 Send 模型尚未加入，本切片继续使用原 output_track_id 单输出。详见 [路由验证](../verification/reaper-parity/ordinary-track-routing.md)。

## 已实现 schema 19 切片

tracks.main_send_enabled 旧工程默认 true；track_sends 保存全局 SendId、源/目标 TrackId、顺序、volume/pan/mute/phase。Receive 不另存一份数据。外键与 Project 全图验证配合，读写及 Undo 保持身份；schema 18 保留旧单输出与 PanMode。当前 Tap 固定 Post-fader，后续增量迁移扩展。详见 [发送验证](../verification/reaper-parity/audio-sends.md)。

## 已实现 schema 20 切片

track_sends.tap（0 Post-fader、1 Pre-FX、3 Pre-fader）默认 0，受 CHECK 约束，reader 拒绝无效值。旧发送身份/位置/参数与主输出保持；AudioSendParameters 携带类型化 Tap，Undo 与 Snapshot 不另存路径。

## 已实现 schema 21 切片

tracks.is_folder=false、parent_track_id=NULL 为旧工程默认值，Parent 有 FK；Project 验证有序森林及组合路由 DAG。层级修改的 Undo 仅保存 TrackId/父节点/文件夹角色与顺序，不覆盖其他参数/FX。旧显式 output_track_id 优先于隐式 Parent 路径，Bus 不转为 Folder。

## 已实现 schema 22 切片

arrangement_folder_compact 按 TrackId 保存视图模式 0/1/2，旧工程为空（Normal）；与其他 ArrangementViewState 同事务写入。Project 领域/Undo 不包含压缩状态，无效值拒绝保存并回滚；UI 仅恢复仍存在的文件夹，删除或取消角色时清理过期视图条目。

## 已实现 schema 23 切片

tracks.phase_inverted NOT NULL DEFAULT 0 / CHECK 0 或 1，旧工程保留原极性；Snapshot 与领域事件包含相位，Undo 属于 callback-safe mix 参数。视图压缩表与路由结构保持原事务和身份。

## 已实现 schema 24 切片

project_master_mix 的 singleton=1 保存 volume_db / pan；迁移插入 unity/center，旧轨道 ID 和增益不变。领域使用独立且已验证的 MasterMix 与 SetMasterMix，Snapshot/Undo 含 Master，存储与整个工程同事务写入。读取拒绝缺行、无效系数以及无 project_meta 却带非默认 Master 状态的数据库。实时输出控制在路由总和之后、guard/Meter 之前；Freeze 源快照清除 Master 增益以免重复烘焙。

## 已实现 schema 25 切片

`audio_item_fades` 用 ItemId 保存两端手动淡化的小数采样长度及形状 0–6，外键删除级联；事务另显式清理旧行。旧 Item 无行时恢复零长度、形状 1，不改变旧音频。加载拒绝非法形状、非有限/负长度和孤立行，长度可以超出 Item；渲染计算有效长度而不覆盖请求值。完整 Item 复制保留淡化，移动和修剪同样保留。实时发布与独立参数历史已接入；GUI 手势及自动 Crossfade 是后续切片。

## 已实现 schema 26 连续淡化曲率

新增 `audio_item_fade_curves` 表，为每个淡化 Item 保留两端可选的新曲率/S 参数对。无行表示 schema 25 的兼容曲线，getter 返回相同数值并不丢弃旧模式。外键指向 `audio_item_fades`，事务先清理曲率表再清理淡化/Item。空值必须成对，至少一端为新模式，值受 −1..1 范围与领域有限值检查。旧工程只读打开仍拒绝旧 schema；应先经现有可写迁移入口升级，不能在只读打开时悄悄写文件。

## 工程帧率扩展（实施中）

现有读写边界：Core `ProjectSettings`/`ProjectSnapshot`、`Project` 构建/恢复、SQLite `project_meta` 全量事务 checkpoint；UI 只读取公开 Project 设置，修改通过 DawAction。Engine 的采样/PPQ 时钟保持原设置，帧率仅用于时间码显示与输入。MCP 及旧工程读取沿既有设置接口，后续完整导出字段另验。

拟新增 schema 27 的 `project_timecode.frame_rate` 独立单行表整数枚举，默认 30 fps，保留原工程全部轨道/素材/淡化/路由数据。十种受支持原厂预设使用固定编码；分数帧率使用精确有理数，29.97DF 与 ND 分开。迁移在既有版本检查与事务中幂等执行；旧 schema 的只读读取沿既有 RequiresMigration 保护，不进行隐式写入。旧应用拒绝比自身新版本的工程，避免混合版本写坏设置。

回退使用迁移前原文件副本或兼容旧 schema 的只读/导出流程，禁止把已迁移工程的版本号强行降级；本次只扩展，不删除或收缩旧字段。验证覆盖旧 26 工程默认、迁移后重复打开、十种设置 round trip、非法编码拒绝和失败事务回滚；全部通过前不标记为验收完成。

## schema 28 — Project Notes

新增独立 project_notes 单行表：title/author/notes 为非空全文，show_on_load/word_wrap 严格 0/1，旧工程默认空值/false/true。与帧率、Master 和媒体快照共用既有保存事务，Notes 插入失败必须回滚全部删除和写入。迁移为独立 CREATE TABLE IF NOT EXISTS + INSERT OR IGNORE，在 user_version 事务内提交；只读旧文件拒绝迁移且保护原字节。旧版本应用拒绝 28；回滚以升级前备份恢复。

公开 ProjectNotes、Project::project_notes、DawAction::SetProjectNotes 及 Snapshot 恢复。原厂实测 Notes 非媒体 Undo：领域动作与嵌套 Batch 仍原子执行，记录媒体历史时移除 Notes 事件；既有 Redo 保留，后续 Undo 不丢弃说明。Unicode、空行与未编辑行尾保留；读取非法布尔值 fail closed。参见 [实测与验证](../verification/reaper-parity/project-notes.md)。

## schema 43 — B_LOOPSRC source-section start

`audio_items.source_section_start_samples` 保存 bounded loop source 的绝对媒体帧起点，与 `loop_source` 和 `source_section_length_samples` 一起写入快照事务。Schema 42 中 `loop_source=1` 且已有 bounded length 的项，迁移时将当前 `source_offset_samples` 作为起点；这与当时 Copy/Paste 创建的 section 起点一致。非 loop 项三列保持关闭/空值，旧全源 B_LOOPSRC 仍以起点/长度空值表达。

读取校验起点与长度必须同时为空或同时存在；bounded 段长度必须大于零，起点加长度不得溢出，且当前源偏移必须落在段内。运行时循环读取只在后台 worker 解码线程中重启 seekable source；短 bounded section 的缓存上限为 1 MiB，避免实时音频 callback 执行文件 I/O、解码或动态分配。回滚以原数据库备份恢复，不允许手工回退 `user_version`。Schema 41 的旧音频表与 Schema 42 的循环项分别有迁移回归；写入、重开、循环源快照及读写拒绝均由 storage/core 测试覆盖。

## schema 49 — signed audio source offset

`audio_items.signed_source_offset_samples` 是音频 source offset 的权威字段，使用 SQLite signed INTEGER 保存媒体帧，可表示 REAPER 非循环 item 在源起点之前的 Contents Nudge。旧 `source_offset_samples` 列继续保留为非负兼容镜像：写入时存 `max(0, signed_offset)`，读取只使用 signed 列。Schema 48 及更早的已有偏移复制到新列，默认值为 0；迁移与 `user_version` 更新在同一事务内完成。当前表重建/增量补列路径也会确保该列存在。

SQLite 不给新列加引用 `loop_source` 的 CHECK，因为早期 schema 迁移会先删除再添加该字段；项目恢复/Action 校验负责拒绝带负偏移的循环项。Schema 49 中 bounded section 的偏移仍按原文件绝对帧解释；schema 50 将修正为 section 内相对帧。

这是扩展式迁移，不删旧列。回滚到旧程序应恢复迁移前的完整数据库备份；schema 49 的数据库应由旧程序按未来 schema 拒绝。不得只把 `user_version` 改回 48：负偏移无法由旧非负列表达，强制降级会丢失语义。

## schema 50 — bounded source section offset origin

`source_section_start_samples` 继续表示原媒体文件中的绝对帧起点；`signed_source_offset_samples` 对 bounded source 则表示 section 内的有符号帧偏移，与 REAPER take 的 `D_STARTOFFS` 一致。循环 bounded section 要求偏移在 `[0, section_length)` 并只在 section 内绕回。关闭循环后允许 signed 偏移：负偏移先播放静音，再从 section 起点单次播放；非负偏移从 section 内对应位置播放，section 结束后补静音至 item 末尾。无 bounded section 的 whole-file source offset 语义不变。

从 schema 49 迁移时，对每条同时有 section 起点/长度的记录，从 signed offset 以及兼容镜像 `source_offset_samples` 中减去旧 section 起点。旧代码曾把这两种坐标混为一体；旧有效循环项必有 offset 不小于 section 起点，因此转换后循环 offset 仍非负。迁移在独立 schema 事务中完成，不删字段；回滚需要恢复迁移前的完整备份，不能手工回退 `user_version`。Core、Media 与 Storage 覆盖相对偏移、负 pre-roll、section EOF 和 schema 49→50。

## schema 51 — MIDI source loop metadata

`items.loop_source` 与 `items.source_loop_length_ticks` 将 MIDI item's loop-source flag 和固定源循环长度保存在快照与 SQLite 中；MIDI source loop length 与可见 item length 分离，因此 Nudge/trim/resize 不会重写循环边界。新增 MIDI item 默认 loop source 开启，初始源长等于插入长度；Duplicate 与 Split 保留 loop flag 和源长。事件 tick 到 item 时间的映射按源偏移和循环长度计算，不改写原始 MIDI event PPQ。

Schema 50 及更早工程默认迁移为 loop source 开启；源长推断为可见 item 长度与存储事件结束位置中的较大者。旧模型没有保留被 trim 后的原始空源长度，因此空 MIDI item 的历史循环长度无法精确恢复。循环 source length 必须大于零，SQLite boolean 只接受 0/1；未来切换 loop source 的 Action 仍需在 Core 校验状态与长度。

重复事件在 Core 的无分配 tick iterator 上贯穿事件计划、Arrangement preview 和标准 MIDI 导出。播放 callback 不做循环枚举、分配、锁或 I/O；事件计划/导出在离线编译阶段限制为最多 1,000,000 个 MIDI 事件，超限返回错误。回滚使用升级前工程备份并由旧版本拒绝未来 schema，不手工改 `user_version`。

## schema 58 — Audio Send MIDI forwarding

`track_sends.midi_enabled` 是每条 Audio Send 独立的 MIDI 转发开关，严格限制为 0/1；新建 Send 默认开启，以匹配 Linux REAPER 7.82 / Default 7 的 `I_MIDIFLAGS=0`（MIDI All→All）。schema 57 及更早的 Send 迁移时默认开启，避免旧工程打开后丢失原厂默认 MIDI 路径。Snapshot、Undo/Redo 与保存事务共同保留此字段。

工程 MIDI note、CC 与 pitch-bend 事件已各自保存 0..15 通道；All→All Send 按原通道转发。事件计划在控制线程按未静音的 Send 路径生成目标轨道事件与 seek/chase 状态；回调读取预编译列表，不枚举路由、不分配、不锁、不做 I/O。普通 Send MIDI 通道过滤/映射、Bus 过滤/映射及双声道 1/2 / Mono 1/2 音频映射已分别在 schema 71、72、73 加入；硬件端口与更高轨道声道数仍是 P3 后续工作。

## schema 59 — MIDI channel identity

`midi_notes` 增加受 `0..15` 检查约束的 `channel` 列；`midi_controllers` 和 `midi_pitch_bends` 增加同类列，并分别将唯一键扩展为 `(item_id, channel, controller, tick)` 与 `(item_id, channel, tick)`。schema 58 及更早工程迁移到通道 0；迁移逐表识别已有列或缺失表，兼容测试构造的部分旧 schema。Snapshot、复制/分割、MIDI 计划/chase、MCP 编辑与标准 MIDI 导出沿用通道身份。

MIDI Editor 已提供 All/单通道/多通道筛选、Filter 开关及绘制目标通道；音符、CC 与 Pitch Bend 的显示和命中按当前筛选集合处理，新事件使用选择的绘制通道。多通道面板目前只覆盖通道集合与绘制目标，REAPER 的反转/独奏/参数值位置长度事件类型筛选和独立原生窗口仍未实现。普通 Send 的 MIDI 通道与 Bus 过滤/映射已接入；物理硬件 MIDI I/O 以及其他消息类型的更多原厂细节仍开放。

## 已实现 schema 71 切片

`track_sends.midi_source_channel` 和 `midi_destination_channel` 分别保存普通 Send 的 0..15 源筛选与目标重映射；NULL 表示 All 与 Original。两列均带 SQLite 0..15 CHECK，schema 70→71 迁移不改既有 Send 的 MIDI 默认语义。`AudioSendParameters` 校验通道边界，Snapshot/Undo/Redo 与存储往返保留参数。Engine 在控制线程编译逐跳 MIDI 路由过滤/映射，块内事件调度与 seek chase 读取预编译结果；不增加实时回调分配或 I/O。验证和原厂截图见 [MIDI Send 验证](../verification/reaper-parity/midi-sends.md)。

## 已实现 schema 72 切片

`track_sends.midi_source_bus` 与 `midi_destination_bus` 保存普通 Send 的零基 0..15 Bus 过滤/重映射；NULL 源 Bus 表示 All，NULL 目标 Bus 保留进入该 Send 的当前 Bus。schema 71→72 为既有发送新增两个可空且带 0..15 CHECK 的字段，不改变原有 All→All 与通道映射。`AudioSendParameters`、Snapshot/Undo/Redo、Routing/Receive 视图均保留 Bus 设置。Engine 在线程外编译 Bus 路由；事件与 seek chase 携带 Bus。CLAP IPC 以既有字节承载 Bus 并升至协议 7，in-process CLAP 根据可用 note-port 将 Bus 事件投递到相应 port。REAPER 7.82 原厂 probe 验证 bus 过滤/映射，Engine/Core/Storage 回归见 [MIDI Send 验证](../verification/reaper-parity/midi-sends.md)。CLAP note-port 对应 REAPER Bus 的端到端原厂行为仍需进一步验证。

## 已实现 schema 73 切片

`track_sends.source_channels` 与 `destination_channels` 保存 Audio Send 的 source/destination 声道模式：Stereo 1/2、Mono 1、Mono 2。schema 72→73 为旧 Send 新增两个默认 Stereo 且受 `0..2` 检查约束的字段。REAPER 7.82 原厂离线 probe 对全部 9 种组合逐通道测量；Stereo 源折叠至 Mono 目标时取均值，Mono 源复制到 Stereo 目标，Mono 源 Send Pan 保持源电平。Engine 在控制线程预编译声道参数，并在实时回调内完成选择、复制与折叠，不分配、不加锁或做 I/O。UI、原厂读回、实窗截图和回归见 [Audio Send 声道映射验证](../verification/reaper-parity/audio-send-channel-map.md)。当前轨道模型限制为两声道，3 声道以上与任意声道偏移仍未实现。
