# 实时 MIDI 通道身份

2026-10-17；Linux REAPER 7.82 / Default 7 parity 的引擎基础切片。此变更保留实时 MIDI 的零起始通道 `0..15`，不声称已完成工程 MIDI 通道编辑或 Send 通道映射。

`ScheduledMidiEvent` 现在携带通道号。实时解码器按 `(channel, pitch)` 配对 Note On/Off，因此同音高在两个通道同时按住时仍得到各自的 Note ID；Controller Change 与 Pitch Bend 同样保留原通道。in-process CLAP 将通道送入音符 PCKN 及 CC/Pitch Bend MIDI 状态字节，清理活动音符时也使用原通道。隔离 CLAP helper 的固定布局 MIDI 记录以原有两个保留字节中的一字节承载通道，记录仍为 16 字节；协议版本由 3 升为 4，helper 拒绝 `0..15` 之外的通道或非零保留字节。Android MIDI 输出保留状态字节的低 4 位通道；发送端拒绝无效通道。

工程 Note、CC、Pitch Bend 事件现在将 0..15 通道保存在项目快照与 SQLite schema 59；旧工程迁移时默认通道 0。播放计划、seek/chase、分割/复制及标准 MIDI 导出保留事件通道。MCP 的 Note 查询与表达式查询返回通道，新增/编辑/upsert 工具接受可选通道（省略时沿用通道 0 或保留既有音符通道）。Arrangement 新建音符与 MIDI 编辑器中新建 CC/Pitch Bend 当前默认为通道 0；控制器与弯音 lane 尚无分通道显示/选择，拖动既有事件会保留其通道。普通 Send 仍为 All→All 原样转发，源/目标筛选重映射、MIDI Bus 与更多 MIDI 消息类型仍待实现。

回归覆盖：实时 decoder 的同音高跨通道 Note ID 配对、计划/seek-chase 多通道事件、CC/Pitch Bend 通道独立键、标准 MIDI 状态字节、快照往返、schema 58→59 旧工程迁移与 MCP 通道参数验证。

本切片是音频事件数据路径，没有新增或修改桌面控件；因此没有运行 UI 截图比对，也没有原厂 UI probe。P3 `ROUTE-SEND-001` 继续保持 `in_progress`；只有在工程数据持久化、Send 映射及原厂行为验证完成后才扩大验收范围。
