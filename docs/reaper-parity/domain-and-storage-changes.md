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
