# 对齐验收计划

## 判定与证据

对齐矩阵的原子条目状态为：`uninvestigated`、`unimplemented`、`partial`、`verification_pending`、`passed`。`blocked` 字段独立表达依赖阻断，不使未完成条目离开分母。

每项至少有参照证据、AAADAW 证据、明确断言、适用平台和执行结果。类别行只作索引，不计算覆盖率。P0 未封闭前不发布总体百分比；Linux、Windows、macOS 分别报告适用分母和结果，不以某一平台代替其他平台。

`passed` 要求所有适用维度通过；仅编译、源码存在、自动测试通过或有截图均不单独足够。未经用户接受的行为差异或替代实现不能通过。

## 场景 fixtures

| Fixture | 覆盖 |
| --- | --- |
| F00 空工程 | 默认窗口、菜单、禁用、工具栏、布局、焦点、项目设置 |
| F01 Audio | Mono/Stereo/多通道、各采样率、重叠、边界、淡化、Slip、Rate |
| F02 MIDI | 跨边界音符、各通道/事件、Pool、Piano Roll/Event/Inline/Notation |
| F03 Routing | Folder/Send/Tap/Sidechain、多通道、Master、硬件和反馈 |
| F04 FX | 可控测试插件、参数/延迟、GUI、状态、缺失/崩溃/卡死 |
| F05 Time/Automation | Tempo/Meter/Ramp、Marker/Region、全部模式、Pool/Modulation |
| F06 Take/Comp | 循环录音、Punch、Lane、Take Marker、跨轨 Comp、Razor/Ripple |
| F07 Delivery | Matrix、Queue、Wildcard、Tail、格式、Metadata、LUFS/True Peak |
| F08 Video/Script/Control | 视频同步、处理器、脚本动作、虚拟/真实 MIDI/OSC 外设 |
| F09 Failure/Large | 缺媒体、磁盘满、未来 schema、掉线、异常退出、大工程、多屏 |

fixtures 用可生成/可分发素材，注明种子、长度、采样率和校验和。REAPER 与 AAADAW 项目表达相同场景，比较归一化语义，不比较项目文件字节。

## 视觉

按基线 manifest 固定窗口、字体、DPI、内容、播放状态和配置。比较客户端几何、控件大小/间距、菜单结构、字体层级、颜色、焦点/选择/录音/旁路/禁用、指针和 Popup。

P0 重复采样建立系统噪声：系统边框、字体抗锯齿和动态 Meter 分别分析；控件几何和行为使用严格匹配断言。颜色、像素和文本误差阈值必须在实际采样后写入冻结配置，不能使用单一全屏相似度掩盖错位。初始文档不填伪造阈值。

记录原图、差分、区域断言和人工检查结论。同平台动态/系统区域的遮罩清单必须说明理由；不允许扩大遮罩覆盖实际差异。窄桌面、高 DPI、多屏、弹窗、插件 GUI 与错误状态分别检查。

## 行为与输入

为每个原子条目记录初态和完整事件序列（含坐标、按下/释放、Modifier、焦点和必要时间），在两应用回放并比较：选择、Cursor、Time/Loop、工程对象、Transport、窗口和配置。

覆盖入口等价、文本焦点、局部 Action Sections、菜单键盘导航、重复键、拖动取消、修饰键中途变化、多选、对象失效、窗口关闭与 Undo/Redo。避免只断言实现内部字段；从公开 Project 快照、实际窗口状态与输出证明。

## 音频与 MIDI

使用脉冲、阶跃、已知正弦、静音、可控延迟/增益插件和固定 MIDI 事件验证路由、Tap、Pan Law、Phase、Fade、Loop、Punch、Rate、PDC、自动化与 Render。先验证帧数、边界、事件顺序和时序，再检查数值误差。

同一可控算法用 Null/Difference；不同编码、SRC、Stretch 或专有 DSP 以事先确定的行为/音质指标验证并记录差异，不能声称位级等同。P0 为每类算法和测试插件冻结容差。Monitor FX 不应进入最终导出，Input/Take/Track/Master 顺序要有路径证明。

真机记录设备、后端、采样率、缓冲、测得延迟、XRuns、监听路径、输入/输出与掉线恢复。无设备测试不能关闭设备项。

## 数据、迁移与失败

每个新增模型测试 Action 校验、事务回滚、Undo/Redo、快照/SQLite 往返和引用稳定性。覆盖旧 schema、未来 schema 拒绝、缺媒体/插件、取消后台任务、磁盘满、异常退出、WAL 恢复和迁移备份恢复。

真实流程：New → Import/Record → Edit Audio/MIDI/Take → FX/Route/Automation → Playback → Render → Save/Close/Reopen → Undo/Redo（按历史语义）→ MCP 查询/修改。MCP 保持既有授权边界，不能绕过 Project 接口。

## 实时安全与性能

审查每个修改的 callback 路径，配合分配探针和故障测试证明无堆分配、阻塞锁/I/O。测量空/普通/大工程的 UI 帧时间分布、拖动延迟、滚动、PCM/插件/队列峰值内存、渲染吞吐和设备 underrun。

P0 固定硬件、fixture、插件、Buffer、测量方法和预算；优化前后比较同环境。不得用更简单工程、关闭插件或扩大缓冲掩盖回归。

## 构建与测试

文档修改只验证链接、矩阵和 diff；功能切片运行相关证明，并按仓库要求以 nextest 执行 Rust 测试：

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo check --workspace --all-targets
cargo xtest
cargo check --workspace --all-targets --features "aaadaw/jack-backend aaadaw/pipewire-backend"
```

Windows WASAPI、macOS CoreAudio、Android 的编译、安装与运行按现有 CI 和平台文档执行。新增后端/格式的 CI 随相应切片补充；真实运行、签名/安装和设备验收分别报告。局部测试通过后不重复无关测试；新失败、修改或剩余风险才扩大验证。

## 最终退出

- P0 全量清单已封闭，所有适用原子条目 `passed`，没有隐藏占位、未解决依赖或未批准差异。
- 全部阶段结果、平台实际验证范围、精度/性能数据和证据可复现。
- 迁移可恢复，音频线程安全无阻断，完整工作流通过。
- 产品文案、教程与已知限制和真实结果一致；唯一 PR 有清晰审阅入口。

任何未满足项保留待完成状态，不用“计划已批准”或“文档已完成”替代产品验收。
