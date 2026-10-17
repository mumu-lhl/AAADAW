# 实时 MIDI 通道身份

2026-10-17；Linux REAPER 7.82 / Default 7 parity 的引擎基础切片。此变更保留实时 MIDI 的零起始通道 `0..15`，不声称已完成工程 MIDI 通道编辑或 Send 通道映射。

`ScheduledMidiEvent` 现在携带通道号。实时解码器按 `(channel, pitch)` 配对 Note On/Off，因此同音高在两个通道同时按住时仍得到各自的 Note ID；Controller Change 与 Pitch Bend 同样保留原通道。in-process CLAP 将通道送入音符 PCKN 及 CC/Pitch Bend MIDI 状态字节，清理活动音符时也使用原通道。隔离 CLAP helper 的固定布局 MIDI 记录以原有两个保留字节中的一字节承载通道，记录仍为 16 字节；协议版本由 3 升为 4，helper 拒绝 `0..15` 之外的通道或非零保留字节。Android MIDI 输出保留状态字节的低 4 位通道；发送端拒绝无效通道。

未带通道身份的既有工程计划事件、预览音符、CC、Pitch Bend 与导出路径显式使用通道 0。它们的工程数据模型和 SQLite schema 尚未增加通道列，打开旧工程行为不变。完整的持久化事件身份、UI 通道选择、普通 Send 源/目标筛选重映射、MIDI Bus 与更多 MIDI 消息类型仍待实现。

回归覆盖：实时 decoder 的同音高跨通道 Note ID 配对、不同通道 CC/Pitch Bend、通道相关 Pitch Bend 字节、CLAP IPC 非零通道往返编码和无效通道拒绝。验证结果：全 workspace `cargo check --all-targets` 通过；Nextest **1396/1396、0 skipped**；workspace all-target Clippy `-D warnings`、`cargo fmt --all -- --check` 和 `git diff --check` 通过；nightly Cranelift 桌面构建通过。当前只安装了 `x86_64-unknown-linux-gnu` Rust target，Android 专用分支尚未交叉编译。

本切片是音频事件数据路径，没有新增或修改桌面控件；因此没有运行 UI 截图比对，也没有原厂 UI probe。P3 `ROUTE-SEND-001` 继续保持 `in_progress`；只有在工程数据持久化、Send 映射及原厂行为验证完成后才扩大验收范围。
