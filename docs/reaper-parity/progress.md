# 对齐进度

## 会话记录

| 字段 | 状态 |
| --- | --- |
| 用户批准 | 2026-10-09；Linux / REAPER 7.82 / 默认主题与配置 |
| 调研提交 | `8cff888a44b67590f0969a3ece82d4ad9e9e0dba` |
| 唯一开发分支 | `feat/reaper-parity` |
| 唯一 PR | [#282](https://github.com/mumu-lhl/AAADAW/pull/282)，Draft；后续阶段只更新此 PR |
| 当前交付 | 批准计划、固定版本动作目录、逻辑键组合与多绑定首批实现、自动和 GUI 验证 |
| 产品覆盖率 | 尚不可计算：P0 原子清单未封闭 |

## 阶段

| 阶段 | 状态 | 证据 / 下一步 |
| --- | --- | --- |
| 文档落地 | 已完成 | [主实施计划](../plans/reaper-parity-implementation.md)；只表示计划已写入 |
| P0 | 进行中 | [实机动作目录](../verification/reaper-parity/linux-7.82/README.md)已采集；界面/鼠标/偏好和验收预算未封闭 |
| P1 | 进行中 | [快捷键基础能力](../verification/reaper-parity/keyboard-foundation.md)；Section、完整 Action List、数字键盘/物理键和事务仍待推进 |
| P2 | 进行中 | Desktop/Touch profile 已分离；Arrange/Mixer/Media Browser 同时显示、独立分割比例及布局持久化；浮动 Docker 等仍待开发 |
| P3 | 进行中 | 普通轨道输出目标、接收路径 Solo、默认 0 dB 声像与旧工程策略迁移；并行 Post-fader Send/Receive 基础已实现；Tap/通道/文件夹等仍待开发 |
| P4 | 未开始 | 基础编辑 |
| P5 | 未开始 | 高级编辑与 Take/Comp |
| P6 | 未开始 | 传输、录音和设备 |
| P7 | 未开始 | MIDI |
| P8 | 未开始 | 自动化 |
| P9 | 未开始 | 插件 |
| P10 | 未开始 | 媒体、工程和渲染 |
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
