# Project Settings General（阶段实现）

固定参照 Linux REAPER 7.82 / Default 7 / factory。General 页原厂默认 BPM 120、Time signature 4/4，与[原厂设置窗口的数值记录](project-settings-general-reference.json)互证。本页当前编辑起始 BPM 和起始拍号；变更只存工程草稿，OK 通过 `SetTempo {start_tick:0}` 与 `SetTimeSignature {start_tick:0}` 一次事务提交，Cancel 丢弃。输入须是有限正速度和可由当前 PPQ 表示的正整数拍号，失败保持窗口与草稿并让工程不变。合法联合提交可一次 Undo。

有限控制集反映已接入的项目状态，不虚列不可用控件。General 尚有原厂 sample rate/whole-sample 强制、BPM base、project/track/item timebase、MIDI timebase、project start time/measure、base ruler spacing、playback/render resampling、pitch shift mode/parameters；这些依赖采样时钟/timebase和工程配置，不在此阶段冒充完成。新配置 REAPER ReaScript 探针确认 sample rate 44100 且未强制、工程 item timebase 代码值 1（Beats: position, length, rate）、timebase flags 0、项目起点偏移 0、起始 120 BPM 与 4/4。其余屏幕值见[数值记录](project-settings-general-reference.json)。视频页显示预设值与非原厂输入仍待实施；Media、Advanced、模板和其余设置页继续按 P10 推进。

真实 AAADAW X11 验证：默认 120/4/4；编辑 96/3/8 时工程不变；OK 后重新打开读取新值；取消窗口后在主窗口 Ctrl+Z 一次还原；0 BPM Apply 时窗口保留、工程不变。证据截图仅来自 AAADAW：[默认](../verification/reaper-parity/project-settings-ui/general-default.png)、[草稿](../verification/reaper-parity/project-settings-ui/general-draft.png)、[确认后](../verification/reaper-parity/project-settings-ui/general-after-apply.png)、[撤销后](../verification/reaper-parity/project-settings-ui/general-real-undo.png)、[拒绝无效输入](../verification/reaper-parity/project-settings-ui/general-invalid-result.png)。本阶段不提交 REAPER 原厂截图或工程。
