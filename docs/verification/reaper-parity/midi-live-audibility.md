# MIDI 实时可听性回归

2026-10-09，P3 基础正确性续项；不是 MIDI Send 或完整 MIDI 输出验收。

新增隔离合成器回归在修改前失败：构建时静音的轨道没有音符进入 MidiEventPlan，播放中解除静音仍无声。初始其他轨道 Solo 的排除同样存在。

AudioRenderGraph 现在使用保留全部来源事件的内部计划，乐器持续获得时间线事件；音频由实时 mute/solo 与 Parent/Send 路径决定可听性。公开 MidiEventPlan::compile 维持原静态过滤契约。调用方 MIDI 输出按当前可听性过滤 Note On/控制变化，Note Off 与 Sustain Off 保留，避免丢失已经发出的音符释放；报告返回实际输出数量，容量错误仍在推进传输前检验。

回归涵盖初始静音/初始 Solo 排除、解除后持续音恢复、再次静音及解除、seek 到音符结束后无残留；输出回归涵盖解除静音后未来 Note On 和随后静音时 Note Off。773 项 workspace 测试全部通过，Clippy warnings denied 通过，隔离恢复 20 次 stress iterations 通过，现有回调零分配测试通过。首次全量的两项 X11 窗口失败来自环境重启后显示服务器退出；重启 Xvfb 后全量复验成功。

尚未实现 MIDI Send、Bus/Channel/硬件路由、外部输出的即时 All Notes Off 与解除排除后的完整状态追赶。是否停止处理静音 FX 的默认偏好及 CPU 策略需后续实机采集并在 P9 对齐；本续项只证明 AAADAW 动态控制不再永久丢失内部乐器时间线，不宣称该偏好与 REAPER 一致。
