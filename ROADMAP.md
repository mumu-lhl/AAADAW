# AAADAW 实现路线图

> 本路线图以 [`docs/design/AAADAW_System_Architecture_Design.md`](docs/design/AAADAW_System_Architecture_Design.md) 为依据，将目标架构拆解为可逐步交付、可验证的里程碑。事项状态以勾选项为准；本路线图不承诺日历日期，按依赖关系与退出标准推进。

## 推进原则

- **先做可用的垂直切片，再扩展功能面**：优先打通“工程数据 → 音频引擎 → UI → 保存/恢复”链路。
- **实时安全是发布门槛**：音频回调不得分配堆内存、获取阻塞锁或执行阻塞 I/O；不得以功能进度为由降低此要求。
- **Action 是唯一写入口**：GUI、快捷键和 MCP 均通过同一套类型化 Action 修改工程状态，并共享校验、撤销/重做与事务语义。
- **先桌面、后移动；先 CLAP、后 VST3**：MVP 面向 Linux/Windows 桌面；Android、VST3 和进程隔离不阻塞 MVP。
- **性能目标以基准测试验证**：在建立可重复基准前，不把“100k 音符 / 144Hz”“降低 90% CPU”等设计目标视作已达成事实。
- **每个里程碑都应能独立构建、测试并演示**；未满足退出标准的工作不算完成。

## Iced 开发参考

本机 Iced Book 文档位于 `/home/mumulhl/data/Projects/iced-book/src`（不属于本仓库，也不是构建依赖）。开始 UI 实现时，按选定的 Iced 版本查阅其中的 `architecture.md`、`the-runtime.md`、`layout.md`、`widgets.md`、`renderers.md`、`concurrency.md`、`subscriptions.md` 和 `shells.md`；可从 `README.md` 或 `SUMMARY.md` 查看文档导航。实现时应以项目实际依赖版本为准，避免本地文档与所用版本不一致。

## Phase 0 — 项目奠基与关键决策

### 目标
建立可持续开发的 Rust 工程，并在开始依赖底层技术之前消除架构中的高风险歧义。

### 工作项

- [x] 建立 Cargo workspace、基础 crate 边界，并为 `aaadaw-core` 建立统一 Action 错误类型；模块边界保持精简。
- [ ] 统一日志约定，并为后续 crate 制定一致的错误/日志接入规范。
- [x] 配置格式化、Clippy、nextest 单元测试和 Linux CI；添加本地开发命令与 nextest 约定。
- [ ] 增加 Windows 构建检查，并记录各平台本地开发与打包步骤。
- [x] 固定 Rust edition/MSRV 声明（Rust 2024 / 1.85）并配置 stable 工具链。
- [ ] 编写 ADR，明确首个音频后端、Iced 与 wgpu 版本策略、实时线程模型及第三方依赖许可审核流程。
- [x] 编写 ADR，确定 `DawAction` 的校验、原子提交、事务回滚、撤销/重做及单调分配轨道 ID 的语义。
- [ ] 明确事件历史与持久化快照的关系，以及双时基的内部表示。
- [ ] 设计 `.aaadaw` 的 WAL 打开、关闭、检查点、复制/导出和崩溃恢复流程。SQLite WAL 工作期间可能产生旁路文件；必须明确何时及如何得到可迁移的单文件工程。
- [x] 决定 MVP 的 MIDI 发声路径：采用 CLAP 乐器宿主，最小宿主能力前置到 Phase 4，完整插件管理留在 Phase 5（[`docs/adr/0005-clap-midi-instrument-path.md`](docs/adr/0005-clap-midi-instrument-path.md)）。
- [ ] 核对 Apache-2.0/MIT 与计划使用的音频、UI、插件、时伸缩依赖的许可和分发条件；VST3 另设法律/许可检查门槛。

### 退出标准

- Linux 上可从干净检出构建并运行最小应用；CI 执行格式、静态检查、测试和构建。
- 上述关键 ADR 已记录决定、替代方案和未决风险；Windows CI 能构建应用骨架。

## Phase 1 — 桌面 Core MVP

### 1. 领域模型、Action 与工程存储

- [x] 实现稳定 ID、轨道、MIDI Item 和 MIDI 音符的基础领域模型。
- [x] 实现轨道静音/独奏状态和撤销/重做，并纳入当前开发 schema。
- [x] 实现 sample-clock 锚定的 `AudioItem`，媒体引用保持不透明，并支持 Action、Undo/Redo、快照和 SQLite 往返；Storage 可将引用解析到工程内音频资产。
- [ ] 实现自动化和路由引用领域模型。
- [x] 实现类型化 `DawAction`、现有操作的输入验证、原子批量事务、撤销/重做和 Action 历史；失败事务不得留下部分状态。
- [x] 实现采样位置与 PPQ tick 互换、960 PPQ/48 kHz/120 BPM 默认值，以及可通过 Action 修改的分段恒定 Tempo Map。
- [x] 实现拍号地图、仅允许小节线变更，并支持 tick 到小节/拍位置查询。
- [x] 实现线性 BPM 渐变，并通过 schema migration v2 持久化 tempo curve。
- [ ] 增加贝塞尔/对数 BPM 曲线、长时间轴精度验证与时基属性测试。
- [x] 建立 `rusqlite` 存储 crate，完成 `.aaadaw` schema v1–v3、`PRAGMA user_version` 事务迁移、未来 schema 版本拒绝，以及 Project 快照保存/加载。
- [ ] 扩展 schema 支持自动化和插件状态，并按工程规模优化全量快照写入；音频资产内容与解码器头部元数据已用当前开发 schema 的附加表存储，不提升 schema 版本。
- [x] 实现新建数据库、原子快照保存/加载、关闭时 WAL checkpoint，并验证保存—关闭—重开状态往返。
- [ ] 验收磁盘空间不足时的保存与恢复行为。
- [x] 验收异常退出后的 WAL 恢复及正常关闭后的单文件复制/迁移（[`docs/tickets/2026-10-02-project-wal-recovery.md`](docs/tickets/2026-10-02-project-wal-recovery.md)）。

**退出标准**：领域层可在无 UI/音频设备环境下测试；Action、批量回滚、撤销/重做、时基换算和数据库迁移均有自动化测试；测试工程可保存并重开且语义一致。

### 2. 实时音频引擎与基本传输控制

- [x] 实现可选 Linux JACK 立体声输出后端，校验采样率/回调块上限，并通过 SPSC 命令队列控制播放/停止。
- [x] 建立 `aaadaw-engine` crate，提供从轨道状态预编译混音系数、无分配/锁/I/O 的单声道轨道到立体声混音原语。
- [x] 编译 MIDI 音符的 Note On/Off 事件计划，支持按半开 sample block 无分配查询，并遵循轨道 mute/solo。
- [x] 实现音频线程本地 Transport 播放状态、采样位置、Seek 与 block 游标；控制线程命令队列和设备时钟仍待接入。
- [x] 增加内存中单声道 PCM 片段的线性重采样播放原语；立体声/多声道和高质量 SRC 仍待实现。
- [x] 使用 `rtrb` 建立固定容量 SPSC PCM 队列；音频线程读取不阻塞，欠载补零并报告欠载帧数。
- [x] 实现固定拓扑 PCM 流混音回调：预分配轨道 scratch、停播静音、运行时统计欠载；AudioItem 按 sample-clock 起止位置调度独立 PCM 流，多 Item 可混入同轨。
- [x] Seek 进入已开始消费的 AudioItem 时，在不推进 Transport/消费 PCM 的情况下拒绝渲染并要求控制侧重填该 Item 的流；refilled feeder 可从 Item 开始重解码并丢弃至目标 sample，渲染图可绑定从指定 sample 起始的队列。
- [x] 将控制侧 Seek、素材解析/后台 refill 与渲染图替换打通；JACK 回调通过 SPSC 队列换图，旧图与 feeder 在控制线程安全回收。长素材随机定位成本仍待优化。
- [x] 可选 JACK 输出后端接入播放/停止与设备采样率检查。
- [x] 将后台 packet 解码、单声道下混、跨 packet 线性重采样和有界 SPSC 队列背压串成媒体流 feeder；音频回调仍不执行解码、锁或 I/O。
- [x] 新建 `aaadaw-app` 控制层，解析 AudioItem 的嵌入/外链媒体引用，等待后台源打开、启动 feeder 并编译固定渲染图；支持 sample seek refill、JACK 图替换、实际 PCM 渲染与缺失外链报错。
- [ ] 扩展 JACK 输入/录音、原生 PipeWire 与 Windows WASAPI；明确每个平台的设备枚举、热插拔和延迟语义。
- [ ] 实现播放/停止、设备参数协商、固定块处理、轨道增益/声像/静音/独奏及基础 Master 输出。
- [ ] 采用静态拓扑分层调度作为 MVP 起点；预分配音频缓冲与 scratch 空间，控制线程和音频线程间通过无锁队列/只读快照交接状态。
- [ ] 实现 MIDI 事件调度、Seek 后 CC 状态追逐、停止/跳转时 Note Off 与 Panic 复位。
- [ ] 实现 Master 输出安全保护与欠载（XRun）可观测性；明确保护器的算法、延迟和安全边界，不宣传其可替代硬件/听力保护。
- [ ] 建立可重复的音频基准与回归测试；对音频回调中分配、锁和 I/O 做审计或检测。

**退出标准**：支持设备上可稳定播放多轨测试工程；回调遵循实时安全约束；设备不可用、XRun、停止和 Seek 均可控，不导致死锁或挂起音符。

### 3. 桌面 UI 与基本编辑

- [x] 用 Iced 0.14 建立桌面窗口、基础文件/编辑/轨道命令入口和固定 Arrangement 主窗口；底部常驻 transport，工程/音频路径使用原生文件选择器；状态修改只经 `Project.apply(DawAction)`。
- [x] 按 `DESIGN.md` 将展开式命令面板重做为紧凑的 REAPER 式 File/Edit/View/Insert/Item/Track/Actions 菜单栏；菜单只列出现有可执行命令。基础 Actions 搜索与菜单操作已接入，完整可配置 Action 注册表仍待后续实现。
- [x] 接入后台音频文件导入、进度/取消及完成后的 AudioItem Action 放置；导入追加到工程首条轨道，需先保存工程。
- [x] 接入可选 JACK 的播放/停止、播放头采样显示和后台 Seek refill；重建图时保留原播放状态。
- [x] 建立首个可停靠面板 Media Browser（[`docs/tickets/2026-10-02-dock-media-browser.md`](docs/tickets/2026-10-02-dock-media-browser.md)）。
- [x] 扩展 transport 至可选原生 PipeWire 后端，与 JACK 并列构建；播放/停止、seek refill、播放头和后端选择均复用现有传输路径（[`docs/tickets/2026-10-02-pipewire-playback.md`](docs/tickets/2026-10-02-pipewire-playback.md)）。
- [x] 建立空间化 Arrangement：Audio/MIDI Item 按轨道和音乐时间显示；拍号感知标尺、水平缩放/平移、轨道/item 选择、edit cursor、垂直轨道滚动、可调 TCP 分割和 Inspector。AudioItem 保留 sample-clock 精确输入、撤销微调、复制/删除及设备 transport 定位；MIDI 保留 Item/音符列表与可撤销编辑和 1/16 量化。
- [x] 支持 Audio/MIDI Item 多选、1/16 网格吸附、跨轨道拖放，以及每次拖动一次撤销/重做；内容、来源偏移和无效放置保护均已验证。
- [x] 增加时间选区和切分（A3.1/A3.2：[`docs/tickets/2026-10-02-arrangement-time-selection.md`](docs/tickets/2026-10-02-arrangement-time-selection.md)）；精确 sample-clock 起点编辑和当前 track/item 选择、edit cursor 已可用。
- [x] 为 Arrange Audio/MIDI Item 增加右键上下文菜单（[`docs/tickets/2026-10-02-arrangement-item-context-menu.md`](docs/tickets/2026-10-02-arrangement-item-context-menu.md)）。
- [x] 为 MIDI Item 增加独立可撤销的重复命令（[`docs/tickets/2026-10-02-duplicate-midi-item.md`](docs/tickets/2026-10-02-duplicate-midi-item.md)）。
- [x] 实现轨道创建/删除/重命名/排序，以及音量、声像、静音和独奏控件；状态变更通过 Action 并支持撤销。
- [x] 增加 Undo/Redo、工程打开/保存和 JACK 播放/停止快捷键；键盘与按钮走相同应用消息，文本控件已消费的按键不触发全局快捷键。
- [x] 将快捷键设置迁入菜单打开的小型 Settings 窗口；用按键捕获替代文本输入，支持清除单项、恢复默认及冲突反馈（[`docs/tickets/2026-10-02-configurable-keyboard-shortcuts.md`](docs/tickets/2026-10-02-configurable-keyboard-shortcuts.md)）。
- [x] 固定主窗口为 Arrangement，将 Media 收敛为 Media Browser 面板、移除 Project 子页面，并在 File 菜单加入新建工程命令（[`docs/tickets/2026-10-02-arrangement-only-main-window.md`](docs/tickets/2026-10-02-arrangement-only-main-window.md)）。
- [ ] 将 Settings 改为左侧类别栏与右侧设置面板；快捷键列表为每个 Action 提供独立的恢复默认入口（[`docs/tickets/2026-10-02-settings-category-navigation.md`](docs/tickets/2026-10-02-settings-category-navigation.md)）。
- [x] 实现音频波形的基础显示（[`docs/tickets/2026-10-02-arrangement-audio-waveforms.md`](docs/tickets/2026-10-02-arrangement-audio-waveforms.md)）；缓存、多级降采样和大工程性能优化按后续里程碑推进。

**退出标准**：用户能在 UI 中建立工程、创建轨道、编辑 Item、播放并保存；关键编辑可撤销/重做；缩放和平移不会触发全量工程数据重算。

### 4. 音频导入、录音与 MIDI 编辑

- [x] 新建 `aaadaw-media`，以 Symphonia 实现 packet-based 文件解码；输出带采样率/声道的 interleaved f32 chunks，并可由后台 feeder 下混/重采样后送入引擎 SPSC PCM 队列。
- [x] 通过当前 schema 的附加表，以固定大小 SQLite BLOB chunks 内嵌音频内容；支持不可变引用、独立 seekable reader、媒体 worker 流式解码，以及原始路径/SHA-256 源文件变化检查，不提升 schema 版本。
- [x] 增加显式外链、重定位、单项/工程级打包为嵌入 Asset；引用保持稳定，打包时快照当前文件内容。
- [x] 增加后台导入、工程级外链打包及源文件状态扫描 worker，汇报进度且支持取消；短事务分批暂存，取消/失败清理未发布素材，完成后原子发布，并提供崩溃残留清理接口。
- [x] 通过 Symphonia 探测容器/codec、采样率、声道、位深、帧数和时长，并将头部元数据持久化到当前开发 schema。
- [x] 源文件变化后重新导入会创建新 Asset；用可撤销的 `EditAudioItem` 更新现有放置引用，旧快照保持不变。
- [x] 增加后台导入的 UI 进度/取消和完成后 placement；目前追加到首条轨道。
- [x] 接入源文件扫描、外链工程级打包的进度/取消 UI；显示素材源状态，并可为缺失的 live external link 重新指定文件。
- [x] 为选中的 Audio Item 提供嵌入快照源变化后的重新导入操作（[`docs/tickets/2026-10-02-reimport-changed-embedded-audio.md`](docs/tickets/2026-10-02-reimport-changed-embedded-audio.md)）。
- [ ] 实现实时录音链路：设备回调经预分配 SPSC 队列传递 PCM，后台线程写入 RF64（或经 ADR 选定的等效格式），保存可恢复的录音元数据。
- [ ] 计算并补偿输入/输出设备报告的延迟；明确设备未提供可靠延迟数据时的行为。
- [x] 实现 MIDI Item 插入/移动/删除、音符插入/删除、网格量化等核心 Action，并接入原子历史与撤销/重做。
- [x] 实现 MIDI 音符 pitch、tick、duration、velocity 编辑 Action，并接入撤销/重做。
- [x] 实现 MIDI Item 移动/调整长度 Action；收缩 Item 时拒绝裁掉已有音符，并支持撤销/重做。
- [ ] 实现钢琴卷帘、音符选择 UI 和可听的 MIDI 播放工作流；当前仅有列表式音符编辑，尚无 MIDI 发声路径。按 Phase 0 决策通过 CLAP 乐器打通播放；插件管理 UI 和完整状态恢复仍留在 Phase 5。
- [x] 为轨道保存可撤销的 CLAP 乐器引用，兼容既有工程（[`docs/tickets/2026-10-02-track-instrument-assignment.md`](docs/tickets/2026-10-02-track-instrument-assignment.md)）。
- [ ] 实现录音准备、输入监听和虚拟键盘基础路径；录音不得在音频回调中进行文件操作。

**退出标准**：可导入音频、录制麦克风素材、编辑并播放 MIDI；模拟异常退出后录音仍能按既定恢复策略读取；基础编辑无爆音/咔哒声回归问题。

### 5. CLAP 插件与参数自动化基础

- [ ] 通过 `clack` 实现进程内 CLAP 插件发现、加载、处理、卸载和基本参数控制。
- [ ] 将插件参数变更接入 Action；提供 begin/perform/end 手势语义，为自动化录制保留一致的接口。
- [ ] 记录插件状态和必要元数据到工程；处理缺失插件、加载失败和状态恢复失败。
- [ ] 显示进程内插件的风险提示；MVP 不承诺插件崩溃隔离，第三方插件不得被误认为运行在安全沙盒中。

**退出标准**：可加载并使用许可兼容的 CLAP 效果器/乐器，保存并恢复工程状态；插件不可用时工程仍可打开并允许用户恢复操作。

### 6. Action 系统与 MCP 原生控制

- [ ] 建立稳定 Action ID、分类、搜索、快捷键映射和基础宏触发机制。
- [ ] 实现本地 MCP 服务，优先提供 STDIO 通道；查询和修改都调用应用层接口，不直接访问 UI 内部状态或绕开 Action 校验。
- [ ] 分批实现设计文档中的工具：创建轨道、插入 MIDI 音符、视口范围内查询音符、量化、设置自动化点、录音准备和触发 Action。
- [ ] 实现只读资源 `daw://project/structure` 与轨道 MIDI 摘要；查询必须有范围/数量上限，避免返回整个大型工程。
- [ ] 写入请求支持事务、参数验证和可理解的错误；录音、破坏性编辑等操作提供明确的用户授权/确认策略。
- [ ] 为 MCP 增加协议错误、无效 ID、越界输入、事务回滚和并发修改测试；记录本地服务的信任边界。

**退出标准**：MCP 客户端可查询工程并通过同一 Action/事务路径修改工程；非法批量请求全部回滚；MCP 查询不会阻塞音频回调。

### 7. MVP 集成与发布候选版

- [ ] 完成 Linux/Windows 桌面打包、配置/日志位置、设备选择和首次启动流程。
- [ ] 完成端到端验收：新建 → 导入/录音 → 编辑音频/MIDI → 播放/混音 → 保存/重开 → Undo/Redo → MCP 查询/修改。
- [ ] 增加示例工程、用户文档、已知限制、故障诊断和插件兼容说明。
- [ ] 验证干净安装、缺少音频设备、缺少插件、磁盘空间不足、非法工程文件和异常退出路径。
- [ ] 按真实使用反馈修复阻塞性问题；公开性能基准和当前支持的平台/插件范围。

**MVP 发布门槛**：核心工作流在 Linux 与 Windows 构建并通过验收；无已知数据损坏或音频线程安全阻断问题；未实现的设计能力清楚标为“不支持”，不得暗示完整 REAPER 功能对等。

## Phase 2 — 生态与跨平台扩展

按 MVP 的实际瓶颈选择顺序，不要求以下子项并行启动。

### 插件隔离与兼容性

- [ ] 实现独立 `aaadaw-scanner`，插件扫描崩溃不影响主进程，并持久化扫描失败/黑名单信息。
- [ ] 设计并实现 `aaadaw-plugin-host`、共享内存音频/MIDI 缓冲和控制 IPC；验证插件崩溃、卡死、重启和版本不匹配处理。
- [ ] 实现隔离插件的独立原生 GUI 窗口；保留高信任插件的可选进程内模式并显式提示风险。
- [ ] 在许可审查通过后，通过 `cxx` 增加 VST3 桥接；VST3 SDK/分发合规是独立发布门槛。

### 平台扩展

- [ ] 完成 macOS 桌面音频设备与打包适配，并把平台能力纳入 CI/验收矩阵。
- [ ] 建立 Android 应用壳与 SAF 虚拟文件系统，打通 `.aaadaw` 打开/保存。
- [ ] 接入 AAudio/Oboe 与 Android MIDI；实现前台录音服务、锁屏保活和设备权限/生命周期处理。
- [ ] 适配触控交互、虚拟修饰键条和触控 Hitbox；评估大小核调度与 Performance Hint，必须以设备测量支撑。

### 音频性能与媒体管线

- [ ] 实现异步峰值缓存（`.aaapeaks`）、波形多级降采样和大工程视口查询。
- [ ] 实现音频预读、素材缓存和内存使用上限；验证磁盘欠载时的恢复行为。
- [ ] 实现 Track Freeze、后台离线渲染、导出队列和明确的抖动（Dither）/浮点输出策略。
- [ ] 只有在基准证明静态调度成为瓶颈后，才实现动态 Work-Stealing DAG；必须保持音频线程无锁/无分配约束。
- [ ] 在评估许可、延迟和音质后集成 Rubber Band 时伸缩能力。

## 后续专业制作能力（分批立项）

这些能力属于设计文档中的高级功能矩阵，不是 Core MVP 的隐含验收条件。应在核心产品稳定后根据用户需求拆分并独立验收。

1. **路由与混音深化**：2–64 通道池、Send/Receive 与多种 Tap Point、侧链、插件引脚矩阵、路由矩阵、全图 PDC；最后再评估反馈路由及其延迟语义。
2. **编辑与自动化**：Razor Editing、Automation Items、Pooled MIDI、更多淡化曲线、非破坏性处理、自动化模式与参数调制。
3. **录音与交付**：Retroactive MIDI、输入 FX、实时量化、冻结/渲染增强、LUFS/True Peak 分析、批量/分区渲染、BWF/iXML 等元数据。
4. **专业工作流**：Project Bay、Track Manager、完整停靠系统、撤销历史视图、控制器映射、MCU/HUI/OSC 与硬件外设。
5. **空间音频与远程协作**：环绕声/Ambisonics、网络音频/MIDI、Web Remote；网络功能须另做身份验证、授权和暴露面审查。

## 全阶段质量门槛

- **领域正确性**：Action/事务、撤销重做、时基边界、工程迁移和崩溃恢复均有自动化测试；对时基互换和序列化往返增加属性测试。
- **实时安全性**：代码审查明确音频回调边界；回调禁止堆分配、阻塞锁和 I/O；XRun、设备断开及关闭流程有回归测试。
- **数据可靠性**：覆盖 WAL 恢复、检查点、异常退出、空间不足、版本迁移及 `.aaadaw` 打包/复制；用户数据迁移前不得静默破坏旧工程。
- **协议与安全**：MCP 工具参数验证、读写范围、批量事务、权限提示和查询上限明确；协议线程不得持有实时音频所需资源。
- **性能可测量**：保留音频负载、工程规模、UI 帧时间和内存使用的基准数据；性能优化需注明硬件、设置和可复现步骤。
- **平台透明**：对外公布每个平台的构建、运行、音频后端和插件支持状态，不把“可编译”当作“已支持”。

## 主要风险与跟踪项

| 风险 | 缓解方式 / 决策门槛 |
| --- | --- |
| 进程内 CLAP 插件可能拖垮宿主 | MVP 显示风险并妥善处理加载失败；隔离能力明确列入 Phase 2，不虚称沙盒保护。 |
| SQLite WAL 的旁路文件与“单文件工程”预期冲突 | Phase 0 定义打开态、关闭态、复制和交付流程，并做断电/崩溃恢复验证。 |
| 跨平台音频设备与录音延迟差异 | 先限定已验证平台和后端；每个平台单独测量设备延迟及 XRun 行为。 |
| 超大 REAPER 功能清单使 MVP 失焦 | 仅以 Core MVP 章节作为首个发布验收；高级功能需独立排序与验收。 |
| VST3、Rubber Band 等依赖存在许可/分发条件 | 集成前完成书面许可审查，并将合规作为发布阻断项。 |
| Android 生命周期、权限与实时能力差异 | 在桌面 MVP 稳定后以真机原型验证；不将桌面实现直接视作 Android 支持。 |
| MCP 自动化可能执行危险操作或放大错误 | 范围化查询、严格参数验证、原子事务、显式授权与可审计错误反馈。 |

## 架构文档追溯

- 领域模型、Action、双时基：设计文档第 3–4 节。
- 实时引擎、调度、PDC 和主输出保护：第 5 节。
- 插件宿主与隔离：第 6 节。
- 录音、预读、冻结与峰值缓存：第 7 节。
- Iced/wgpu、MIDI 编辑与触控：第 8–9、13 节。
- MCP：第 10 节。
- `.aaadaw` / SQLite：第 11 节。
- 通用轨道、路由与高级制作能力：第 14–15 节。
