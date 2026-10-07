# AAADAW: 架构设计与技术规范白皮书

*(Advanced Audio Architecture Digital Audio Workstation)*

## 1. 项目概述与核心哲学

### 1.1 项目定位

AAADAW 是一款使用 Rust 编写的现代化、跨平台（桌面 Linux / Windows /
macOS 及移动端
Android）、高可靠性开源数字音频工作站（DAW）。其设计立足于两项核心基石：

1.  **全面继承 REAPER 的工业设计哲学**：极致轻量、超低 CPU
    损耗、统一轨道模型（Universal Track Model）、无缝的
    Action/快捷键映射体系以及高精度的 MIDI 编辑体验。

2.  **AI 原生集成与自动化编曲**：原生实现 Model Context Protocol (MCP)
    服务端，允许 AI Agent
    深度理解工程乐理结构，通过结构化事务执行编曲、改曲、混音与录音控制。

### 1.3 分阶段演进路线图 (Phased Evolution Roadmap)

- **Phase 1 (Core MVP)**：聚焦于 Desktop-first
  (Linux/Windows)、进程内原生 CLAP 插件宿主
  (clack)、静态拓扑分层调度器、Iced + iced::widget::shader (wgpu) 混合
  UI 架构、Action/MCP 原生集成以及实时麦克风录音。

- **Phase 2 (Ecosystem Expansion)**：引入 VST3 兼容桥接
  (cxx)、独立进程沙盒隔离、Android 跨平台移植 (AAudio/SAF) 以及动态
  Work-Stealing DAG 调度器。

### 1.2 关键约束

- **实时安全性（Real-Time
  Safety）**：音频回调主线程绝对禁止任何形式的动态堆内存分配（Heap
  Allocation）、系统互斥锁（Mutex Locking）与阻塞 I/O 操作。

- **崩溃隔离（Crash Resilience）**：第三方 C/C++ 插件（CLAP /
  VST3）运行于独立进程沙盒中，插件的段错误或死锁不危害 DAW 核心。

- **无损跨平台（Universal Portability）**：工程采用 .aaadaw 单文件
  SQLite 事务数据库封装，在桌面端与 Android 端双向迁移无损，内部包含音频
  Chunk BLOB 与相对引用，彻底消除文件路径断裂问题。

## 2. 系统总体分层架构

┌────────────────────────────────────────────────────────────────────────┐

│ Presentation & Control │

├───────────────────────────────────┬────────────────────────────────────┤

│ GUI Layer (Desktop & Android) │ AI Agent Control Layer │

│ - Iced + iced::widget::shader (wgpu) Hybrid UI Architecture │ - MCP
Server (JSON-RPC) │

│ - Iced Shell, Mixer & Retained Controls│ - Viewport Scoped Queries │

│ - wgpu Instanced Timeline Canvas │ - Transactional Command Dispatch │

│ - Virtual Modifier Bar (Touch) │ - Transactional Command Dispatch │

├───────────────────────────────────┴────────────────────────────────────┤

│ Application & State Core │

├────────────────────────────────────────────────────────────────────────┤

│ - Event Sourcing Engine (Atomic DawAction Stream, Undo/Redo) │

│ - Dual Timebase Engine (Sample Accurate \<\-\--\> PPQ Ticks) │

│ - Tempo Map, Meter Envelopes & Curve Interpolation │

│ - Virtual File System (VFS, Android SAF & Desktop POSIX/Win32) │

│ - .aaadaw Single-File Transactional Database Engine (rusqlite/WAL) │

│ - Dual Timebase Engine (Sample Accurate \<\-\--\> PPQ Ticks) │

│ Real-Time Audio Engine │

├────────────────────────────────────────────────────────────────────────┤

│ - Static Topological Layered Parallel Dispatcher (Phase 1 MVP) │

│ - Dynamic Work-stealing DAG Scheduler (Phase 2 Milestone) │

│ - Graph-wide Plugin Delay Compensation (PDC) │

│ - Master Brickwall Safety Limiter (Hearing/Acoustic Protection) │

│ - SIMD Peak / RMS Metering Pipeline │

├───────────────────────────────────┬────────────────────────────────────┤

│ Plugin Sandboxing Layer │ Streaming & Recording I/O │

│ Plugin Hosting Layer │ Streaming & Recording I/O │

│ - In-Process Native CLAP (clack) │ - Realtime Mic Rec (rtrb Queue) │

│ - Phase 2 VST3 & Sandboxing │ - Background Disk Flush (RF64) │

│ - Shared Memory Ringbuffers │ - Background Disk Flush (RF64) │

│ - Native CLAP (clack) & VST3 cxx │ - Async Stream Prefetcher │

│ - Subprocess Scanner & Blacklist │ - Track Freeze & Offline Bounce │

├───────────────────────────────────┴────────────────────────────────────┤

│ Platform Drivers Layer │

├────────────────────────────────────────────────────────────────────────┤

│ - Audio: PipeWire / JACK (Linux), WASAPI (Win), CoreAudio via CPAL (macOS), AAudio/Oboe (Android)
│

│ - MIDI: ALSA / PipeWire (Linux), WinRT (Win), Android JNI MIDI │

└────────────────────────────────────────────────────────────────────────┘

## 3. 核心领域数据模型与事件溯源（CQRS）

### 3.1 命令模式（Command / Action Pattern）

系统内所有对工程状态的修改均通过严格类型化的 DawAction
驱动，杜绝随意修改共享可变状态：

这里的“工程状态”指轨道、Item、路由等可编辑领域状态。音频资源字节由 Storage
单独管理；导入失败后，只有在数据库确认没有已保存的 audio item 引用时，Storage
才可清理尚未放置的资源。这类回滚/垃圾回收不创建 DawAction，因为资源尚未进入
可撤销的工程内容。已被 Item 引用的资源仍遵循工程快照和撤销生命周期。

pub enum DawAction {

// 轨道操作

CreateTrack { index: usize, name: String },

DeleteTrack { track_id: TrackId },

SetTrackVolume { track_id: TrackId, volume_db: f32 },

SetTrackPan { track_id: TrackId, pan: f32 },

// MIDI 与 Item 操作

InsertMidiItem { track_id: TrackId, start_tick: u64, length_ticks: u64
},

DuplicateMidiItem { item_id: ItemId },

SplitMidiItem { item_id: ItemId, split_ticks: Vec<u64> },

AddMidiNotes { item_id: ItemId, notes: Vec\<MidiNote\> },

DeleteMidiNotes { item_id: ItemId, note_ids: Vec\<NoteId\> },

QuantizeItem { item_id: ItemId, grid: GridFraction, strength: f32 },

// 录音与监听

ArmTrack { track_id: TrackId, armed: bool, input_channel: InputRouting
},

SetMonitoringMode { track_id: TrackId, mode: MonitoringMode },

// 事务封装

BatchTransaction { tx_id: u64, actions: Vec\<DawAction\> },

}

### 3.2 事务与回滚控制

- 所有的批量修改（特别是 AI Agent 通过 MCP 提交的大批量乐理调整）封装在
  BatchTransaction 中。

- 事务执行前留存状态快照引用（通过基于持久化数据结构的不可变树）。

- 若序列化验证、时钟校验失败或发生中途异常，直接触发
  Rollback，保证状态不会处于残缺的中间态。

## 4. 时基、节拍与时钟映射系统

### 4.1 双时基体系（Dual Timebase）

- **物理时基（Time /
  Samples）**：用于录制音频、环境拟音素材，其在绝对时间轴上的位置不随
  BPM 改变而漂移。

- **乐理时基（Beats / PPQ Ticks）**：通常以 960 PPQ（Pulses Per Quarter
  Note）为基准，MIDI 音符、鼓点切片等严格跟随节拍网格缩放。

### 4.2 动态变速轨（Tempo Map）

- 支持小节拍号变更（如 4/4 拍无缝切 7/8 拍）与 BPM
  渐变曲线（线性、贝塞尔、对数）。

- 提供全局只读纳秒级精度的换算二叉搜索树： \$\$\\text{SampleIndex} \\iff
  \\text{PPQTick}\$\$

- 当前阶梯/线性 TempoMap 使用 `f64` 积分锚点，tick 和 sample 的绝对位置限制在
  `2^53` 以内以保证整数可精确表示；超出范围的转换或 tempo 锚点会返回范围错误。
  若需要扩大该范围，应先改为拆分整数/小数锚点并增加长时间属性测试。

- **弹性音频（Time-Stretching）**：音频素材变速不变调时，流式读取管道接入集成
  Rubber Band 算法库，实时计算弹性因子。

### 4.3 MIDI 事件追逐（CC & Event Chasing）与复位保护

- **事件追逐机制（Event Chasing）**：当用户在时间轴进行 Seeking
  或跳转时，引擎自动向后追溯并扫描当前位置之前的 MIDI 控制器状态（如
  CC64 延音踏板、CC1 调制轮和 14-bit Pitch Bend 弯音），并在新位置立刻补发
  最新状态。Pitch Bend 作为独立于 7-bit CC 的事件存储；无历史弯音时发送中心值
  8192，避免反向 seek 后残留旧弯音。

- **复位与 Panic 保护**：跳转或停止播放时，自动向所有激活通道发送 Note
  Off、All Notes Off (CC 123) 与 All Sound Off (CC 120)
  消息和中心 Pitch Bend（8192），避免音符挂起（Stuck Notes）和弯音状态泄漏。

## 5. 实时音频图与无锁调度引擎

### 5.1 调度器设计与实时安全原则

- 静态拓扑分层调度器（MVP 阶段）：为避免过早的并发复杂性，MVP
  阶段采用稳健的静态拓扑分层并行调度器；将动态 Work-Stealing 保持为
  Phase 2 的高级里程碑。

- 音频线程生命周期内 **零堆内存分配**，使用提前分配的全局 AudioArena
  充当临时 Scratch 缓存。

- 摒弃操作系统级别的 std::sync::Mutex，线程间同步全部依赖：

  1.  rtrb（单生产者单消费者无锁环形队列）。

  2.  arc-swap / triple_buffer（只读拓扑与状态更新）。

### 5.2 全图延迟补偿（Graph-wide PDC）

- 引擎在拓扑排序阶段递归分析所有支路（包含多重发送 Send、侧链 Sidechain
  及并行 Bus）。

- 统计各分支累积预读延迟（Lookahead
  Latency），计算延迟差并在较快通路上自动插入延迟线（Delay
  Line），确保所有音频流到达母带（Master）时在 1
  个采样点精度内完全对齐。

### 5.3 母带数字样本峰值保护（Master Sample-Peak Guard）

- 当前实现始终在所有 PCM、乐器和轨道效果器汇总后、交给 JACK/PipeWire
  之前执行 sample-peak ceiling。用户可在 Audio Settings 中设为 -12 至 0 dBFS，默认
  -1 dBFS；高于 ceiling 的有限样本直接夹限，NaN/Infinity 替换为静音，并累计供 UI
  读取的计数。
- 音频回调只做一次原子读取和有界逐样本处理；不分配、不加锁、不进行 I/O。ceiling
  由设置线程通过单次原子写入更新。
- 这是最终数字 sample-peak guard，不是响度归一化器，也不是带 look-ahead/oversampling
  的 true-peak limiter；瞬时硬夹限可能产生失真，重构后的模拟/true peak 仍可能超过
  sample ceiling。它不保证扬声器声压或听力安全。
- 若未来需要透明的 true-peak limiting，应将其作为独立 DSP 能力设计和测量，明确其
  latency、oversampling、release 与 bypass 行为，不得把当前 guard 描述为该能力。

## 6. 插件宿主与沙盒隔离系统

### 6.1 格式支持与技术栈

- **CLAP**：作为一等公民（First-class）优先支持的插件格式，采用原生 Rust
  框架 clack 深度集成，默认采用进程内（In-Process）加载运行模式（对标
  REAPER 架构）。

- VST3 与沙盒隔离：VST3 桥接 (cxx) 及独立进程沙盒隔离机制延后至 Phase 2
  逐步实现。

### 6.2 独立进程沙盒（Out-of-Process Sandboxing）

- **宿主-工作进程架构**：每一个（或每一组）第三方插件运行在子进程
  aaadaw-plugin-host。

- **低延迟 IPC**：

  - 音频块（Audio Buffers）与 MIDI 事件流通过 OS 共享内存（POSIX Shared
    Memory / Windows File Mapping）传递。

  - 控制信号通过高速本地套接字（Unix Domain Socket / Named Pipes）交互。

  - 插件崩溃时仅 Worker 进程终止，DAW
    提示"插件崩溃已隔离"，用户可一键重启该节点。

### 6.3 独立进程扫描器（Plugin Scanner CLI）

- 启动时扫描插件目录的操作交由独立 CLI aaadaw-scanner 完成。

- 扫描单个插件崩溃时，主程序将异常路径记录进 blacklist.json
  并平滑跳过，彻底规避因单插件损坏导致 DAW 无法启动的问题。

### 6.4 插件 GUI 窗口呈现架构

- **独立原生浮动窗口**：跨进程沙盒下的插件 UI
  采用独立原生浮动窗口呈现，绕过 Wayland、macOS 等现代操作系统在跨进程
  Surface 嵌入与系统事件传递上的严格限制。

- **进程内加载开关（In-Process
  Toggle）**：针对高信任度、高性能要求的插件，提供可配置的进程内（In-Process）直接加载选项，以进一步降低渲染与交互开销。

## 7. 录音与音频流式子系统

### 7.1 录音链路物理隔离

1.  **音频采集**：平台输入驱动（AAudio / PipeWire /
    WASAPI）在实时回调中获取 PCM 帧。

2.  **入队**：通过 rtrb 环形缓冲直接推入，无任何系统调用。

3.  **独立落盘线程**：后台低优先级线程按页对齐批量消费，直接追加写入
    RF64 / Wave64 格式。

4.  **断电保护**：定期将已写字节长度安全刷新至持久化元数据，确保非法断电下音频录制
    Take 100% 可读。

5.  **硬件延迟对准**：计算驱动报告的输入延迟与输出延迟之和，停止录音生成
    Item 时，将时间戳向前自动偏移消除系统漂移。

### 7.2 流式预读与冻结机制

- **多轨流式读取（Disk
  Streaming）**：后台预读器维护环形预读缓冲区（Prefetch
  Buffer），维持内存占用低于阈值。

- **音轨冻结（Track Freeze）**：针对算力占用过高轨道，一键离线渲染为
  32-bit Float WAV 并挂起插件，释放 90% 以上 CPU 负载。

- **超实时离线渲染（Offline Bouncing）**：解除硬件时钟等待，以 CPU
  最大多核负载执行离线计算混音，并挂载 TPDF Dither 抖动算法。

### 7.3 切片微淡化与异步峰值缓存（Peaks Cache）

- **自动切片微淡化（Micro-fading /
  Crossfading）**：在音频切片（Slice/Item）边界自动施加 2-5ms
  的微淡入淡出及交叉淡化，避免非零过零点（Non-Zero
  Crossing）导致的爆音与咔哒声（Clicks/Pops）。

- **异步二进制峰值缓存（.aaapeaks）**：后台异步线程解析音频并生成多级
  Mipmap 二进制峰值文件（.aaapeaks），UI
  渲染层通过内存映射（mmap）快速读取，彻底消除时间线缩放与平移时的磁盘
  I/O 瓶颈。

## 8. UI 渲染与跨平台交互架构

### 8.1 架构设计：双层解耦混合架构 (Dual-Tier Decoupled Hybrid Architecture)

- 外壳控制层 (Outer Control Shell)：采用 Iced 框架（基于 Elm
  Architecture / TEA 架构），保持纯 Rust 工具链依赖与纯粹的 MIT
  开源协议。负责窗口管理、顶部 Transport 播放控制条、左侧 Track
  Headers（静音/独奏/录音准备按钮、推子控件）、通过 iced_split
  实现的可停靠面板、Action 搜索弹窗以及对话框。

- 核心编辑视口 (Core Editing Viewports)：通过 iced::widget::shader
  特性直接嵌入到 Iced 的布局树中，承载自定义的 wgpu WGSL 渲染管线。

### 8.2 实例化与降采样图形管线 (Instanced & Decimated Graphics Pipeline)

- MIDI 钢琴卷帘 (MIDI Piano Roll)：基于 draw_indexed_indirect GPU
  实例化技术渲染，单次 Draw Call 即可在 144Hz 刷新率下轻松处理 10
  万+音符。

- 波形降采样金字塔 (Waveform Decimation
  Pyramid)：根据每像素采样率缩放级别，匹配读取 Min/Max
  多级降采样数据（.aaapeaks）。

- 统一矩阵投影 (Uniform Matrix Projection)：时间线主体的缩放和平移通过更新 GPU
  Uniform 投影矩阵完成，不重算轨道和 Item 等静态几何。可见波形是例外：视口或缩放改变时，
  按可见素材帧范围查询 Min/Max 金字塔，只更新可见波形实例缓冲；不重建无关工程几何。

- 自动化包络线 (Automation
  Envelopes)：贝塞尔曲线求值直接在顶点着色器（Vertex Shader）中完成。

### 8.3 音频生态组件复用与参数手势协议

- 音频生态复用：参考并复用开源 Maolan 项目中 maolan-widgets
  的可复用模式（包括与 symphonia 集成的
  AudioClip、OctaveKeyboard、MIDIClip），以最大化降低开发成本。

- 参数手势协议 (Parameter Gesture Protocol)：将 Vizia/nih-plug 中的
  begin_edit / perform_edit / end_edit 参数手势协议融合并引入至 Iced 的
  Action 模型中，实现精确的自动化包络捕捉。

### 8.2 交互模式适配（桌面 vs 移动端）

- **桌面端**：1:1 对标 REAPER
  经典键鼠快捷键（Alt+拖拽切片、Ctrl+拖拽复制、套索选择等）。

- **Android 触控端（虚拟修饰键系统）**：

  - 在屏幕边缘常驻可自定义的 **Virtual Modifier
    Bar**（悬浮修饰键条：选择、画笔、切片、橡皮、力度调节）。

  - 引入 **Hitbox Padding
    触控包围盒**，小尺寸音符自动扩展判定区域，确保触控屏精准点选。

## 9. 移动端 Android 专项系统适配

1.  **AAudio 低延迟独占流**：

    - 显式指定 INPUT_PRESET_UNPROCESSED，彻底规避 Android 系统通话降噪与
      AGC 压限对人声和乐器的频响破坏。

2.  **前台保活服务**：

    - 注册 FOREGROUND_SERVICE_TYPE_MICROPHONE 并持有
      PARTIAL_WAKE_LOCK，防范锁屏休眠挂起渲染主线程。

3.  **虚拟文件系统（VFS）**：

    - 封装 Android Storage Access Framework
      (SAF)，统一抹平桌面标准路径与 Android content:// URI 的差异。

4.  **异构大小核（big.LITTLE）调度与线程亲和性**：

    - 通过 sched_setaffinity 将实时音频回调与调度线程绑定至 CPU
      性能大核（Big Cores）。

    - 接入 Android Performance Hint
      API（ADynamicPerformanceWorkload），动态向系统反馈音频帧工作量，防止系统误判降频引发热限频与音频欠载（XRun）。

## 10. MCP (Model Context Protocol) 规范

### 10.1 通信通道

- 本地后台开启独立异步服务，通过标准输入输出（STDIO）或本地 Socket
  监听客户端接入。

### 10.2 MCP 核心工具集（Tools）

- daw_create_track(name, track_type, color)

- daw_insert_midi_notes(track_id, item_id, notes: \[ { pitch, tick,
  duration, velocity } \])

- daw_scoped_query_notes(track_id, measure_start,
  measure_end)：支持视口局部查询，防止 Token 爆炸。

- daw_quantize(track_id, item_id, grid_type)

- daw_set_automation_point(track_id, parameter, tick, value, curve)

- daw_arm_recording(track_id, input_source)

- daw_trigger_action(action_id)：直接调用内部 Action 编号执行宏。

### 10.3 MCP 只读资源（Resources）

- daw://project/structure：获取全局轨道名称、BPM、拍号骨架概览。

- daw://project/track/{id}/midi_summary：获取特定轨道的音符密度与和弦走向摘要。

### 10.4 乐理计算助手与异步进度通知

- **声明式乐理计算助手（Music Theory Tools）**：内置和弦转 MIDI
  音符、音阶音级查询等确定性乐理计算工具，避免 LLM
  在复杂乐理推演中产生幻觉。

- **流式进度通知（Progress Notifications）**：针对 Track Freeze、Offline
  Bouncing 等耗时较长的大任务，通过 SSE 或异步通道实时向 AI Agent
  推送百分比进度与状态变更通知。

## 11. 基于 SQLite 事务引擎的单文件工程规范 (.aaadaw)

### 11.1 架构设计哲学与反模式辨析

在数字音频工作站（DAW）的工程存储设计中，基于 Git 或纯文本（如大
JSON/XML）的差量对比与文本存储被证明是一种"反模式"（Anti-Pattern）：

- 人类不可读性与冲突不可解性：数万个包含 Tick
  偏移、浮点自动化包络、密集的 MIDI 列表的 JSON
  文本在发生分支冲突时，人类用户或普通文本 Merge
  工具根本无法理解并手动修复。

- 序列化与解析延迟高：大型工程文本反序列化耗时极大，且存在浮点数文本化转换的精度漂移（Floating-point
  Drift）。

- 移动端续航杀手：在 Android 设备上频繁对数十 MB 的 JSON
  进行解析和全量重写极度消耗内存与 CPU 算力，对电池续航造成毁灭性打击。

- 协议与持久化解耦：MCP 接口传输协议（如
  JSON-RPC）与磁盘持久化存储格式完全解耦，磁盘存储必须追求极高的 I/O
  效率与事务安全性。

### 11.2 单文件容器结构与页级差量写入 (Single-File Container & Page-Level Incremental Writes)

- 单文件容器（Application File Format）：工程被封装为单一的
  MySong.aaadaw 文件（采用 SQLite
  作为应用文件格式），彻底消除了散落文件夹与资源相对路径断裂问题。

- 页级差量写入（Page-Level Incremental / Diff
  Writes）：修改单个音符、推子或自动化点时，仅将受影响的 4KB B-Tree
  物理页写入 WAL（Write-Ahead Log）日志中，写入延迟低至
  0.1ms，相比全量重写减少数千倍的磁盘写入量。

### 11.3 关系型表结构设计 (Relational Schema Organization)

工程内部数据库采用严密的 SQL 关系表组织，主要结构包括：

- project_meta：存储全局配置（BPM、拍号、采样率、时基设定等）。

- tracks：存储轨道层次结构、音量、声像、颜色及路由关系。

- items：存储时间轴上的音频片段与 MIDI Item 的位置、长度及增益设置。

- midi_notes：包含高密度音符数据（Pitch, Tick, Duration, Velocity,
  NoteId）。

- automation_envelopes：存储参数自动化控制点与曲线插值类型。

- plugins：存储插件链配置与私有状态快照（.state 二进制块 Blob）。

- audio_assets：内嵌音频资源切块（通过增量 BLOB I/O 读写 Chunk BLOBs）。

### 11.4 存储引擎与文件系统调优 (Storage Engine & Filesystem Tuning)

- WAL 模式强约束：强制启用 PRAGMA journal_mode = WAL，将随机写入转化为对
  WAL 日志的顺序追加写入，对 SSD 极度友好。

- 4KB 物理页对齐：强制配置 PRAGMA page_size = 4096，精准对齐 Linux /
  Btrfs 文件系统及 SSD 硬件 4KB 物理扇区。

- 同步与 Checkpoint 策略：使用 PRAGMA synchronous = NORMAL 与 PRAGMA
  wal_autocheckpoint = 1000，平衡吞吐性能与落盘安全性。

- Btrfs / CoW 兼容与碎片整理：明确无需关闭 Btrfs 的
  CoW（Copy-on-Write）特性（从而完整保留关键的 Btrfs 数据 Bit-rot
  校验和保护），后续可通过标准的 VACUUM INTO
  指令进行干净的线性碎片整理与紧凑压缩。

- 原生 ACID 容灾保护：利用 SQLite 原生的原子提交（Atomic
  Commits）机制实现抗崩溃保护，保证意外断电时零字节损坏与完全可恢复，彻底放弃临时文件重命名（Rename
  Hack）等易错机制。

## 12. 格式解码与合规

- **多格式音频解码**：集成纯 Rust 编写的 symphonia 库，原生支持 MP3,
  AAC, FLAC, OGG, WAV 等流媒体导入，无需庞杂的 C 库依赖。

- **开源合规建议**：

## 13. REAPER 界面排布、交互模式与快捷键全量规范 (REAPER Interface Layout, Interaction & Keybinding Specification)

### 13.1 界面宏观布局与视觉拓扑 (UI Component Topology)

标准的 REAPER 工作区由以下核心区域组成：

- **顶部菜单栏 (Top Menu Bar)**：提供系统的全局功能菜单与配置入口。

- **主工具栏 (Main Toolbar)**：位于轨道控制面板 (TCP)
  上方，提供高频操作的快捷按钮阵列。

- **轨道控制面板 (TCP, Track Control
  Panel)**：位于界面左侧，管理轨道属性、控制与路由。

- **时间标尺 / 标记轨 (Time Ruler / Marker
  lane)**：位于编排时间线上方，显示时间轴刻度与标记点。

- **编排 / 时间线视口 (Arranger / Timeline
  Viewport)**：位于界面中央，呈现媒体块与音频/MIDI 剪辑的主编辑区域。

- **走带控制器 (Transport
  Bar)**：停靠于底部，控制播放、录音、节拍器及时钟显示。

- **调音台控制面板 (MCP, Mixer Control
  Panel)**：可停靠于底部的抽屉式面板，提供多通道推子与混音控制。

### 13.2 主工具栏按钮阵列与动作映射 (Main Toolbar Button Array)

主工具栏从左至右包含 14 个标准 REAPER 默认按钮及其对应功能与快捷键：

- **1. New Project**：新建工程 (Ctrl+N)

- **2. Open Project**：打开工程 (Ctrl+O)

- **3. Save Project**：保存工程 (Ctrl+S)

- **4. Project Settings**：工程设置 (Alt+Enter)

- **5. Undo**：撤销 (Ctrl+Z)

- **6. Redo**：重做 (Ctrl+Shift+Z / Ctrl+Y)

- **7. Metronome Toggle**：节拍器开关（右键打开节拍器设置）

- **8. Auto-Crossfade Toggle**：自动交叉淡化开关 (Alt+X)

- **9. Item Grouping Toggle**：媒体块分组开关 (Alt+Shift+G)

- **10. Ripple Editing Toggle**：波纹编辑模式切换（循环切换：关闭 Off
  -\> 单轨 One Track -\> 全轨 All Tracks，Alt+P）

- **11. Envelope Points Move with Items Toggle**：包络点随 Item 移动开关

- **12. Grid Display Toggle**：网格线显示开关 (Alt+G)

- **13. Snap to Grid Toggle**：吸附网格开关 (Alt+S，右键打开吸附设置)

- **14. Locking Toggle**：工程控件锁定开关（右键打开锁定设置）

### 13.3 轨道控制面板 (TCP) 控件拓扑与交互

每个轨道包含以下细化控件与交互元素：

- **Track Index & Name**：轨道序号与名称。

- **Record Arm button**：圆形红色录音准备切换按钮。

- **Input Monitoring**：输入监听开关（三态切换：关闭 Off / 正常 Normal /
  磁带自动 Tape Auto）。

- **Input Routing**：输入路由选择（单声道 Mono、立体声 Stereo、MIDI
  通道及虚拟键盘 Virtual Keyboard）。

- **Mute (M) & Solo (S)**：静音与独奏控制按钮。

- **Volume Knob/Fader**：带立体声电平表 (Stereo Meter) 的音量旋钮/推子。

- **Pan Knob**：声像旋钮（支持声像规律与模式 Pan Law/Modes）。

- **FX button**：绿色/红色旁路 Toggle 开关，点击打开 FX Chain 插件链。

- **Route button**：I/O 路由矩阵、Send 发送与硬件输出设置。

- **Trim/Envelope button**：自动化包络轨控制（如 V 呼出音量包络，P
  呼出声像包络）。

- **Phase Invert**：相位反转开关。

### 13.4 走带控制器 (Transport Bar) 规范

走带控制器提供完整的播放管理与时钟信息：

- **控制按钮**：Record (Ctrl+R)、Play (Space)、Pause (Ctrl+Space)、Stop
  (Space / Enter)、Go to Start (Home / W)、Go to End (End)、Repeat /
  Loop Toggle (R)。

- **时间显示与参数**：大时钟码显示 (Big Clock: Bars.Beats.Ticks / M:S:ms
  / Samples)、播放速率滑块 (Play Rate: 0.25x - 4.0x，带保持音高开关
  Preserve Pitch Toggle)、BPM Tap & 编辑、拍号 (Time Signature)
  输入、选区起始/结束/长度 (Selection start/end/length) 读数。

### 13.5 编排视口鼠标修饰键 (Mouse Modifiers) 行为准则

**Media Item 媒体块**

- 左键点击：选中 Item / 定位编辑光标

- 左键拖拽：移动 Item（带网格吸附）

- Ctrl + 拖拽：复制 Item

- Alt + 拖拽：滑动编辑 (Slip Edit，保持 Item 外框不动移动内部内容)

- Alt + 拖拽边缘：变速拉伸 (Time-stretch audio)

- Shift + 拖拽：忽略吸附移动

- Shift + Ctrl + 拖拽：忽略吸附复制

**Media Item Fade Handles 淡化手柄**

- 拖拽角手柄：调整 Fade-in / Fade-out

- 拖拽顶边：调整 Item 增益手柄 (Item Volume Gain Handle)

**Arranger Blank Space 编排区空白处**

- 左键点击：定位编辑光标

- 左键拖拽：创建时间选区 (Time Selection)

- 右键拖拽：框选 (Marquee Select) Items

- 中键拖拽：平移/手形滚动 (Hand Scroll/Pan)

**Mousewheel 滚轮操作**

- 滚轮：垂直滚动

- Alt + 滚轮：以光标为中心水平缩放 (Horizontal Zoom)

- Ctrl + 滚轮：垂直轨道高度缩放 (Vertical Track Height Zoom)

- Ctrl + Alt + 滚轮：水平滚动

### 13.6 核心主窗口快捷键对照表 (Main Section Keybindings)

  ------------------------------------------------------------------------
  **分类 (Category)**  **快捷键 (Shortcut)**   **动作说明 (Action
                                               Description)**
  -------------------- ----------------------- ---------------------------
  Transport            Space                   Play / Stop

  Transport            Enter                   Stop / Pause

  Transport            Ctrl + Space            Pause

  Transport            Ctrl + R                Record

  Transport            R                       Loop Toggle

  Transport            Home / W                Go to Start

  Transport            End                     Go to End

  Transport            Left / Right            Nudge cursor

  Transport            \[ / \]                 Set loop points

  Transport            Esc                     Clear time selection

  Editing              S                       Split at cursor

  Editing              Shift + S               Split at time selection

  Editing              D                       Trim left

  Editing              Delete                  Delete item

  Editing              Ctrl + Z                Undo

  Editing              Ctrl + Y                Redo

  Editing              Ctrl + A                Select all

  Editing              Ctrl + C / Ctrl + V     Copy / Paste

  Editing              Ctrl + D                Duplicate item

  Editing              Alt + M                 Mute item

  View / Toggles       Ctrl + M                Toggle Mixer

  View / Toggles       ?                       Action List

  View / Toggles       Alt + Space             Play from selection

  View / Toggles       F2                      Item properties

  View / Toggles       Alt + Enter             Project settings
  ------------------------------------------------------------------------

### 13.7 MIDI 钢琴卷帘编辑器全量规范 (MIDI Editor Specification)

**音符基本操作 (Note Operations)**

- 网格左键点击：插入音符（按默认网格长度绘制）；左键拖拽：插入音符并拖拽调整时长。

- 双击音符 或 Alt+点击音符：删除音符。

- 拖拽音符主体：移动音调/时间位置；拖拽音符边缘：调整音符时长。

- Shift + 拖拽：忽略网格移动音符。

- Ctrl + 拖拽：复制音符。

- 右键拖拽：框选 (Marquee Select) 音符。

**音调与力度快捷键 (Pitch & Velocity Keys)**

- Shift + Up / Shift + Down：音符移调 +1 / -1 半音。

- Ctrl + Shift + Up / Ctrl + Shift + Down：音符移调 +1 / -1 八度。

- Alt + 音符上滚动鼠标滚轮：增大/减小音符力度 (Velocity)。

- Q：打开量化 (Quantize) 对话框。

- Alt + Q：打开人性化 (Humanize) 对话框。

- Ctrl + F2：查看音符属性 (Note Properties)。

**多轨道 CC 编辑器 (Multi-lane CC Editor)**

底部可停靠多控制器通道（包括 Velocity 力度、CC1 Mod 调制轮、CC11
Expression 表情、CC64 Sustain 延音踏板、Pitch Bend 弯音），支持手绘曲线
(Freehand drawing) 与线性渐变 (Linear ramps)。

**动作列表命名空间隔离 (Action List Section Partitioning)**

确保主窗口 (Main Section) 与 MIDI 编辑器 (MIDI Editor Section)
保持独立的动作 ID 命名空间，与 REAPER 严格对齐。

- AAADAW 核心采用 Apache 2.0 / MIT 双许可协议。

## 14. 万能轨道模型与全局路由系统规范 (Universal Track Model & Routing System Specification)

### 14.1 万能单轨模型 (Universal Track Model)

- **REAPER 哲学继承**：无独立轨道类型划分（不人为区分"音频轨"、"MIDI
  轨"、"Aux/Bus
  轨"、"文件夹轨"或"母带轨"）。任何单条轨道均可同时承载音频剪辑 (Audio
  Clips)、MIDI Items、虚拟乐器以及 FX 插件。

- **文件夹母子嵌套树 (Folder Track Hierarchy)**：

  - 缩进子轨即可将上级轨自动转换为文件夹轨。

  - 文件夹轨自动充当子混音 (Sub-mix) 汇总 Bus，无需手动搭建 Aux 轨。

  - 支持多级层级嵌套（文件夹内嵌套文件夹）。

  - 三态文件夹折叠展示：Normal（正常）、Small（紧凑）与
    Hidden（折叠隐藏子轨）。

### 14.2 多通道通道池 (2 to 64 Channels Architecture)

- 每条轨道可动态分配 2, 4, 6\... 直至 64 个独立通道的内部音频通道池。

- **标准侧链范式**：通道 1/2 传输直通节目音频，通道 3/4 传输辅助侧链
  (Sidechain) 控制信号。

- **多输出 VSTi / CLAP 乐器路由**：鼓机与多音色合成器可将分轨 (Stems)
  分别输出至轨道不同通道（1/2 至 Kick，3/4 至 Snare，5/6 至 Hi-Hat
  等）。

- **空间音频与环绕声就绪**：单轨原生支持 5.1、7.1 以及高阶 Ambisonics
  通道。

### 14.3 发送与接收（Sends & Receives）拓扑与抽头位置 (Tap Points)

- **三种独立抽头位置 Routing 阶段**：

  - **Post-Fader
    (Post-Pan)**：标准混响/延迟发送；轨道音量推子与声像变更等比例缩放发送量。

  - **Pre-Fader
    (Post-FX)**：乐手耳机监听辅助混音；电平独立于混音推子控制，但包含所有插入效果器处理。

  - **Pre-FX**：原始干声抽头位置；进入任何插件链之前的输入/Item
    直接引出（用于外部侧链、原始分析或无加工录音分流）。

- **发送/接收属性**：

  - 独立音量 (Volume)、声像 (Pan)、静音 (Mute) 与相位反转 (Phase Invert)
    控制。

  - 通道重映射矩阵：源轨道通道（如 1/2）到目标轨道通道（如侧链用的
    3/4）。

  - MIDI 路由：完整通道转发、过滤（指定 MIDI 通道 1-16）或移调/重映射。

- **Master / Parent Send Toggle**：选框开关，允许轨道绕过母带 Bus
  或父级文件夹，用于隔离专用侧链触发源或直接硬件输出。

### 14.4 插件引脚映射器 (FX Pin Connector Matrix)

- 在每个 CLAP/VST3 插件外壳窗口内部提供专用矩阵网格窗口。

- 支持在轨道通道 (1..64) 与插件输入/输出引脚（Main L/R, Aux/Sidechain In
  L/R, Multi-out Pins）之间进行灵活的交叉连接。

### 14.5 全局路由矩阵视图 (Routing Matrix View, Alt+R)

- 交叉点交互式 2D 矩阵网格：Y 轴为源轨道，X
  轴为目标轨道、硬件音频输出与硬件 MIDI 输出。

- 点击网格可创建/删除 Sends，右键可直接在网格中设置发送电平与抽头位置。

### 14.6 反馈回路路由支持 (Feedback Routing Engine)

- 工程级显式开关："Allow feedback routing"（轨道 A -\> 轨道 B -\> 轨道
  A）。

- 音频 DAG 调度器检测到环路时，在反馈边上自动插入 1-block
  延迟缓冲区，支持声学共鸣与磁带延迟反馈环路，同时防止引擎死锁。

  - VST3 模块作为可选隔离插件运行，优先力推完全开放、无专利风险的
    **CLAP** 作为首选格式。

### 14.7 轨道与路由交互手势与操作规范 (Track & Routing Interaction Gestures & Workflows)

**1. 轨道创建、排布与折叠手势 (Track Management Gestures)**

- Double-click blank TCP area: Instantly create new track (matches
  shortcut Ctrl+T).

- Dragging audio/MIDI files from file explorer to blank TCP area:
  Automatically creates corresponding new tracks and places items.

- Track reordering & Folder creation: Dragging track vertically reorders
  tracks; dragging slightly rightward onto the lower-right edge of a
  target track indents it, instantly transforming the target track into
  a Folder parent track and the dragged track into a child track.

- Folder compacting button: Clicking folder icon cycles through 3
  states: Normal -\> Small (compact) -\> Collapsed (hidden child
  tracks).

- Double-click track name area to edit name; pressing \'Tab\' commits
  and jumps to rename the next track immediately.

- Track height resizing: Dragging bottom border resizes single track;
  holding \'Shift\' + dragging bottom border resizes all tracks
  proportionally.

- Multi-track relative adjustment: Selecting multiple tracks (via
  Ctrl+Click or Shift+Click) links faders and pans proportionally;
  holding Shift temporarily bypasses group linking for individual
  adjustments.

- Double-click volume fader or pan knob: Instantly resets to default
  value (0.0 dB / Center).

**2. 标志性"拖拽路由"交互手势 (Iconic Drag-and-Drop Routing Gestures)**

- Cable Drag-to-Track: Left-click and drag from the \'ROUTE\' button on
  Track A (mouse cursor changes to a patch cable plug icon) and drop
  onto Track B: instantly establishes an A -\> B Send and opens a mini
  routing popup.

- Shift + Drag from \'ROUTE\' button: Instantly establishes a send with
  default settings without popping up the routing dialog.

- Alt + Click on \'ROUTE\' button: Instantly removes all existing sends
  or toggles Master/Parent send.

- Multi-Track Batch Routing: Select multiple tracks, hold \'Shift\' and
  drag the \'ROUTE\' button of any selected track to a destination
  track/bus: batch routes all selected tracks to the destination in one
  single action.

- Quick Sidechain Drag-to-Plugin (一秒侧链手势): Click and drag the
  \'ROUTE\' button of the source track (e.g., Kick) directly onto an
  open plugin window on the destination track (e.g., Compressor on
  Bass): automatically expands destination track from 2 to 4 channels,
  establishes channels 1/2 -\> 3/4 send, and maps plugin sidechain
  detection inputs to Auxiliary 3/4.

**3. 全局路由矩阵 (Routing Matrix, Alt+R) 操作规范**

- Interactive Cross-point Grid: Rows are sources, columns are
  destinations.

- Left-click empty cell: Instantly establishes a send (marked with
  color/dot).

- Left-click active cell: Toggles/removes the send connection.

- Right-click cell: Pops up inline fader and tap-point selector without
  leaving the matrix view.

- Click and drag across a line of cells: Batch establishes continuous
  sends across multiple tracks in a single sweep (e.g., routing 8 drum
  tracks to Drum Bus).

## 15. REAPER 工业级全量高级特性与功能矩阵规范 (REAPER Advanced Industrial Feature Matrix Specification)

### 15.1 现代编辑与素材微操体系 (Advanced Editing & Item Manipulation)

- **Razor Editing (光刀编辑)**：Alt+right-drag 2D rectangular
  multi-track area selection cutting across items and envelope lanes,
  enabling simultaneous moving, stretching, and copying.

- **Automation Items (自动化对象)**：Modular envelope blocks supporting
  loop, stretch, pooling, and cross-track ghost updates.

- **Pooled MIDI Items (幽灵素材池)**：Alt+Shift+drag linked duplicate
  items, updating all instances in real-time.

- **Snap Offset (瞬态吸附偏移点)**：Internal triangular snap anchor on
  items to align grid by transient instead of file boundary.

- **Item-Level Normalization**：Non-destructive normalize to Peak (0
  dBFS), RMS, or Integrated LUFS (-14 LUFS).

- **Non-destructive Operations**：Non-destructive Item Reverse, Heal
  Splits, and Explode/Implode (multichannel to mono, MIDI by pitch,
  tracks to takes).

- **Fade Curves**：7 mathematical fade shapes (Linear, Exp, Log,
  S-Curve, Quarter/Half Sine).

### 15.2 录音与实时捕获子系统 (Advanced Recording & Capture)

- **Retroactive MIDI Recording**：Rolling ringbuffer capturing
  background improvisations, recalled via \'Insert all recently played
  MIDI\'.

- **In-line MIDI Editor (E shortcut)**：Editing notes directly within
  the timeline track without opening a separate window.

- **Input FX Chain & Real-time Input Quantize**：Pre-recording DSP
  printing permanently to disk, and immediate grid alignment on capture.

- **Metronome Routing & Count-in**：1/2/4 bar pre-roll, and dedicated
  hardware channel direct-routing bypassing master bus.

### 15.3 自动化、调制与混音引擎深度 (Advanced Automation, Modulation & Mixing Engine)

- **6 Automation Modes**：Trim/Read, Read, Touch, Latch, Write, Latch
  Preview.

- **Parameter Modulation**：Real-time modulation of any plugin parameter
  via internal LFO, Audio Envelope Follower (sidechain control without
  compressor), and Parameter Linking.

- **VCA Grouping**：VCA master fader controlling slave tracks
  proportionally without altering audio routing.

- **64-bit Double Precision Mixing Engine**：Eliminating tiny sub-bit
  float underflows across large track summing.

- **Pan Laws & Stereo Pan Modes**：0dB, -3dB, -4.5dB, -6dB laws;
  Balance, Stereo Pan with Width (100% to -100%), Dual Pan.

- **Track Wiring Diagram**：2D node-based cable connection graph.

- **ReaSurroundPan**：3D spatial panner for 5.1, 7.1, 7.1.4 Dolby Atmos,
  and Ambisonics.

### 15.4 效果器包装外壳与硬件集成 (FX Wrapper Superpowers & Hardware Integration)

- **Per-Plugin Oversampling**：2x to 16x oversampling per plugin
  instance to eliminate non-linear aliasing distortion.

- **Global Wet/Dry & Delta Solo**：0-100% parallel blend, and Delta Solo
  listening only to what the plugin modifies.

- **ReaInsert**：Hardware outboard gear patcher with single-sample ping
  latency calibration.

- **Monitor FX Chain**：Dedicated hardware monitoring chain (room EQ,
  meter) completely isolated from final render bounces.

- **Protection & Efficiency**：Auto-Mute Overload Protection (+18 dBFS
  speaker/ear protection) and dynamic zero-CPU silent sleep.

### 15.5 工业级资产管理与交付导出 (Asset Management & Industrial Delivery)

- **Project Bay (Ctrl+B)**：Central database managing source media,
  items, FX chains, and groups.

- **Track Manager (Ctrl+Shift+M)**：Spreadsheet table for batch TCP/MCP
  visibility, mute, solo, and freeze.

- **Undo History Window (Ctrl+Alt+Z)**：Multi-step memory-tracked state
  rollback.

- **Four-Way Floating Dockers**：Docking any panel top, bottom, left, or
  right.

- **Wildcard Batch Rendering**：Renaming exports dynamically via
  \$track, \$bpm, \$project, \$marker.

- **Render Queue & Region Render Matrix**：Background batch render queue
  and multi-region stem export.

- **Dry Run Loudness Analysis**：50x offline scan outputting Integrated
  LUFS, LRA, and True Peak reports.

- **Broadcast Metadata Embedder**：BWF timecode, iXML, and ID3 chunk
  injection.

- **Mastering Resampling Filters & Render Modes**：Sinc 64-768pt,
  r8brain filters & 1x Online Render mode.

- **Directory Management**：Clean Project Directory & Track
  Consolidation.

### 15.6 硬件控制、网络协同与外设生态 (Control Surfaces, Networking & Remote)

- **Full-system Action MIDI/OSC Learn**：Mapping any action to hardware
  encoders with relative mode support.

- **Control Surface Protocols**：MCU, HUI, and OSC with motorized fader
  feedback.

- **Web Remote Control**：Integrated HTTP server allowing browser-based
  mobile remote control over Wi-Fi.

- **ReaStream**：Zero-latency uncompressed 64-channel 32-bit float audio
  and MIDI streaming across LAN via UDP.

- **External Clock Lock**：SMPTE LTC & MTC External Clock Lock.
