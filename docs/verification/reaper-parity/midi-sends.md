# MIDI Send 基础（AAADAW-ROUTING-003）

2026-10-17；Linux REAPER 7.82 / Default 7 / factory configuration；`ROUTE-SEND-001` 仍为 `in_progress`。本切片实现 REAPER 普通 Send 的 MIDI `All → All` 默认行为和逐条关闭开关，不表示通道映射或 P3 已完成。

## 原厂参照

[ReaScript 探针](../../../scripts/reaper_parity/probe-midi-send-defaults.lua)在隔离空工程中新建 `MIDI Source → Instrument` Send，读取 `I_MIDIFLAGS=0`、`I_SENDMODE=0`，并保存等价工程。原厂 [Routing 窗截图](images/midi-sends-native.png)中该 Send 的 Audio 行是 `1/2 → 1/2`，MIDI 行是 `All → All`。Lua 输出和默认配置记录在[原厂 TSV fixture](fixtures/midi-sends-native-defaults.tsv)。探针 SHA-256：`1ba169992a1283fa9f94005f3ee68b25d5dadd10e8acd0c8af85854aa47060bf`；fixture SHA-256：`a6e0140227e9c35fec837d1fda7d4a28892120a6912d51cb2695d884eef92f93`。原厂截图右上角显示音频设备关闭；此处只比较路由控件和 Send 默认值。

## 实现和行为验证

`AudioSendParameters.midi_enabled` 独立于音量、Pan 和 Send Mute；默认 true，对应 All→All。Routing 子窗口显示 `Send MIDI · All → All` 复选框，关闭 MIDI 不删除或静音 Audio Send。轨道 MIDI Item 的 Note On/Off、CC、Pitch Bend 经过未静音且启用 MIDI 的 Audio Send 路径后，会在目标 Track 上得到对应事件；路由目标也进入 seek 时的 active-note/controller/pitch-bend chase。并行路径上的 note 副本获得独立 note ID，使 attack、release 与 chase 的 CLAP 身份保持成对。多段 MIDI Send 路径在非实时 `MidiEventPlan` 编译时展开，块内查询只读预编译数组。

现有工程 MIDI 事件没有 MIDI channel 字段，因此 All→All 是当前可表示的范围；筛选源通道、指定目标通道、MIDI Bus、Program/Aftertouch/SysEx 转发仍未实现。被静音的 Audio Send 不转发 MIDI，关闭 `midi_enabled` 只关闭该 Send 的 MIDI 路径。

AAADAW Linux 实窗运行于 Xvfb `:1881`、1280×800。对 `Audio 1` 打开 Routing 窗后，默认复选框可见且勾选；点击后复选框取消勾选，标题标记工程未保存。随后打开 Receiver 的 Routing 窗，Audio 1 Receive 显示 `MIDI Off`，Audio 2 Receive 保持 `MIDI All → All`。三张 [发送侧默认](images/midi-sends-aaadaw-all.png)、[发送侧关闭](images/midi-sends-aaadaw-off.png)及[接收侧状态](images/midi-sends-aaadaw-receives.png)截图记录实际控件和操作结果；它们验证状态与路由关联，不声称像素级等同 REAPER。

AAADAW 桌面二进制使用 nightly Cranelift；Build ID `7f756aec8fb01b77b1ea9ae58d24f5e75709afe0`，SHA-256 `16853573e5cb0e26d892c873488b312da9eacd2eb7ed9d0ab300dc582656eeac`。All、off 和 Receive 截图 SHA-256 分别为 `456145824fca8c54bc269ab23422b3014ff75f503b7ba41aa27e51be55d2389f`、`22da81965262b4ba95f214dcc443ea38fd6b1753e72b79db1a316adc847e597c` 和 `ffdb484422a061660757ed52e4b5c931cd52a85fafd47277b7336bb0459d2b2e`。

SQLite schema 58 增加 `track_sends.midi_enabled`，旧 Send 默认迁移为 true；Project Snapshot、撤销/重做和保存/加载均保留参数。自动回归覆盖新 Send 默认值、开关快照/撤销、schema 57→58 迁移、Send mute/MIDI off、嵌套路由上的 note/CC/bend 与 seek chase，以及并行路径 note ID 的 attack/release 配对。完整 workspace Nextest 1394/1394（0 skipped）和严格 all-target Clippy (`-D warnings`) 通过；`cargo fmt --check` 通过。

## 剩余差异

MIDI 通道字段、通道筛选/重映射和 MIDI Bus 仍未建模。MIDI 事件现会跟随启用的 Folder Parent 主输出并逐层传播，关闭任一层的主发送会截断后续路由；AAADAW 引擎回归见[文件夹验证](folder-parent-routing.md)，REAPER 原厂事件计数见[Folder Parent MIDI fixture](folder-parent-midi.md)。硬件 MIDI I/O、Feedback、PDC、Routing Matrix/Wiring 和完整 Send/Receive 操作布局也未完成。原厂 Undo 历史与主题像素差异没有在本切片验证。`ROUTE-SEND-001`、P3 和全量 REAPER 对齐继续保持未完成。
