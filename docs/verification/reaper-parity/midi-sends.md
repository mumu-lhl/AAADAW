# MIDI Send 基础（AAADAW-ROUTING-003）

2026-10-20；Linux REAPER 7.82 / Default 7 / factory configuration；`ROUTE-SEND-001` 仍为 `in_progress`。本文记录普通 Send 的 All→All 默认、MIDI 开关、源通道与 MIDI Bus 筛选、目标通道映射及目标 Bus 选择；完整路由界面和 P3 仍未完成。

## 原厂参照

[ReaScript 探针](../../../scripts/reaper_parity/probe-midi-send-defaults.lua)在隔离空工程中新建 `MIDI Source → Instrument` Send，读取 `I_MIDIFLAGS=0`、`I_SENDMODE=0`，并保存等价工程。原厂 [Routing 窗截图](images/midi-sends-native.png)中该 Send 的 Audio 行是 `1/2 → 1/2`，MIDI 行是 `All → All`。Lua 输出和默认配置记录在[原厂 TSV fixture](fixtures/midi-sends-native-defaults.tsv)。探针 SHA-256：`1ba169992a1283fa9f94005f3ee68b25d5dadd10e8acd0c8af85854aa47060bf`；fixture SHA-256：`a6e0140227e9c35fec837d1fda7d4a28892120a6912d51cb2695d884eef92f93`。原厂截图右上角显示音频设备关闭；此处只比较路由控件和 Send 默认值。

## 实现和行为验证

`AudioSendParameters.midi_enabled` 独立于音量、Pan 和 Send Mute；默认 true，对应 All→All。Routing 子窗口显示 `Send MIDI · All → All` 复选框，关闭 MIDI 不删除或静音 Audio Send。轨道 MIDI Item 的 Note On/Off、CC、Pitch Bend 经过未静音且启用 MIDI 的 Audio Send 路径后，会在目标 Track 上得到对应事件；路由目标也进入 seek 时的 active-note/controller/pitch-bend chase。并行路径上的 note 副本获得独立 note ID，使 attack、release 与 chase 的 CLAP 身份保持成对。多段 MIDI Send 路径在非实时 `MidiEventPlan` 编译时展开，块内查询只读预编译数组。

工程 MIDI Note/CC/Pitch Bend 已保存 0..15 通道，All→All Send 按原通道透明转发；源通道筛选和目标通道重映射详见下节。REAPER 通道 probe 覆盖 Program Change、Poly Aftertouch 与 Channel Pressure。新到达的实时 MIDI 事件已在调度、CLAP 处理和 helper IPC 中保留通道号，并由 Android 原始 MIDI 输出保留状态字节通道，边界与测试见[实时通道身份验证](live-midi-channels.md)。被静音的 Audio Send 不转发 MIDI，关闭 `midi_enabled` 只关闭该 Send 的 MIDI 路径。

AAADAW Linux 实窗运行于 Xvfb `:1881`、1280×800。对 `Audio 1` 打开 Routing 窗后，默认复选框可见且勾选；点击后复选框取消勾选，标题标记工程未保存。随后打开 Receiver 的 Routing 窗，Audio 1 Receive 显示 `MIDI Off`，Audio 2 Receive 保持 `MIDI All → All`。三张 [发送侧默认](images/midi-sends-aaadaw-all.png)、[发送侧关闭](images/midi-sends-aaadaw-off.png)及[接收侧状态](images/midi-sends-aaadaw-receives.png)截图记录实际控件和操作结果；它们验证状态与路由关联，不声称像素级等同 REAPER。

AAADAW 桌面二进制使用 nightly Cranelift；Build ID `7f756aec8fb01b77b1ea9ae58d24f5e75709afe0`，SHA-256 `16853573e5cb0e26d892c873488b312da9eacd2eb7ed9d0ab300dc582656eeac`。All、off 和 Receive 截图 SHA-256 分别为 `456145824fca8c54bc269ab23422b3014ff75f503b7ba41aa27e51be55d2389f`、`22da81965262b4ba95f214dcc443ea38fd6b1753e72b79db1a316adc847e597c` 和 `ffdb484422a061660757ed52e4b5c931cd52a85fafd47277b7336bb0459d2b2e`。

SQLite schema 58 增加 `track_sends.midi_enabled`，旧 Send 默认迁移为 true；Project Snapshot、撤销/重做和保存/加载均保留参数。自动回归覆盖新 Send 默认值、开关快照/撤销、schema 57→58 迁移、Send mute/MIDI off、嵌套路由上的 note/CC/bend 与 seek chase，以及并行路径 note ID 的 attack/release 配对。完整 workspace Nextest 1394/1394（0 skipped）和严格 all-target Clippy (`-D warnings`) 通过；`cargo fmt --check` 通过。

## 剩余差异

## MIDI Bus 筛选与映射

REAPER 7.82 的 `I_MIDIFLAGS` bits 14–21 保存源 Bus，bits 22–29 保存目标 Bus。原厂 probe 把 Item MIDI 改送到 JSFX bus index 1（Routing 窗显示 Bus 2），测得 All→All 保留该 bus、Source Bus 1 丢弃它、Source Bus 2 通过；目标 Bus 1/2 分别重映射到 JSFX index 0/1。Source Bus 2→Destination Bus 2 仍通过，Source Bus 1→Destination Bus 2 不通过。逐 bus 计数、Routing 窗截图和 capture 配置见[原厂 bus 结果](fixtures/midi-send-buses-native.tsv)、[原厂截图](fixtures/midi-send-buses-native.png)与[probe](../../../scripts/reaper_parity/probe-midi-send-buses.lua)。REAPER 的 Bus 编号从 1 开始，JSFX `midi_bus` 与工程内部 bus 值从 0 开始。

AAADAW 将普通 Send 的源/目标 Bus 保存为零基 0..15；`None` 源 Bus 表示 All，`None` 目标 Bus 保留事件当前 bus，对应 REAPER 的 All 语义。schema 72 为旧 Send 加入受约束的 bus 列，旧工程保持 All→All。Send Routing 控件提供 All/Bus 1–16，Receive 视图显示映射。预编译计划会沿多跳 Send 逐段筛选/改写 bus，并在 Note、CC、Pitch Bend、Program、Aftertouch 和 seek chase 中保留 bus；回归验证 Bus 1→Bus 2→Bus 3、错误源 bus 丢弃及 CLAP helper IPC bus 字节。AAADAW 实窗 Routing 截图和截图校验信息见[bus 控件验证](fixtures/midi-send-buses-aaadaw-evidence.tsv)。

## 源通道筛选与目标通道映射

REAPER 的 `I_MIDIFLAGS` 低 5 位表示源通道（0=All、1–16=指定通道），接着 5 位表示目标通道（0=原通道、1–16=映射通道）。原厂探针在一个 Note、CC、Pitch Bend、Program Change、Poly Aftertouch 与 Channel Pressure 多通道序列上分别测了 All→All、Channel 2→Original、All→Channel 6 和 Channel 2→Channel 6；全部事件类型均只从所选源通道通过，并按目标规则保留或重映射。逐通道计数与解释见[原厂结果](fixtures/midi-send-channel-map-native.tsv)，原厂 Routing 窗截图显示 Channel 2 → Channel 6：[REAPER 7.82](fixtures/midi-send-channel-map-native-after.png)。Lua/JSFX 探针及复现脚本见[probe](../../../scripts/reaper_parity/probe-midi-send-channel-map.lua)、[JSFX](../../../scripts/reaper_parity/jsfx/aaadaw-midi-send-channel-map-probe.jsfx)和[capture 脚本](../../../scripts/reaper_parity/capture-midi-send-channel-map-native.sh)。

AAADAW 的 Send 参数保存零基源/目标通道（`None` 分别代表 All 和 Original）；schema 71 给旧 Send 增加两个可空、0..15 约束列，旧工程保持 All→Original。Routing 面板提供 All/Channel 1–16 和 Original/Channel 1–16 选择项，Receive 显示该 Send 的实际设置。预编译 MIDI 路由会逐跳组合过滤与映射，覆盖 Note On/Off、CC、Pitch Bend、Program Change、Poly Aftertouch 和 Channel Pressure；目标通道同时用于 seek chase 状态。SysEx 不带 MIDI 通道，不受通道筛选或重映射影响。引擎测试覆盖 Channel 2→6→8 的两段路径、其余来源通道丢弃和七种带通道事件/状态追赶；存储测试覆盖映射往返及 schema 70→71 的旧工程默认。nightly Cranelift 桌面二进制在 Xvfb 1280×800 下现场启动，操作 Track 的 IO 按钮打开 Routing 面板；[AAADAW 通道控件截图](fixtures/midi-send-channel-map-aaadaw-channels.png)展示 Channel 2 → Channel 6，原始和面板操作截图、Linux 7.82 原厂结果与哈希记录见[证据清单](fixtures/midi-send-channel-map-evidence.tsv)。

实时与工程 MIDI 通道字段边界见[通道验证](live-midi-channels.md)。Folder Parent 主输出的逐层转发见[文件夹验证](folder-parent-routing.md)和[原厂事件计数](folder-parent-midi.md)。CLAP note-port 与 REAPER MIDI Bus 的端到端等价仍需独立原厂验证；硬件 MIDI I/O、Feedback、PDC、Routing Matrix/Wiring 和完整 Send/Receive 操作布局也未完成。原厂 Undo 历史与主题像素差异没有在本切片验证。`ROUTE-SEND-001`、P3 和全量 REAPER 对齐继续保持未完成。
