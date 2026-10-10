# AAADAW-ROUTE-001

参考：固定 Linux REAPER 7.82、Default 7、隔离配置。用 48 kHz/16-bit mono 常量素材（Source=0.125，Receiver=0.25，长度 1s），去除 Item 自动淡化；Source 关闭 Master Send，仅发送到 Receiver；Master stereo WAV 24-bit 渲染，采样第 24000 帧。未使用物理音频设备。

| 场景 | REAPER 实测 L/R |
| --- | --- |
| 发送增益为零，仅接收端自身素材 | 0.25 / 0.25 |
| Solo Source（in place） | 0.125 / 0.125 |
| Solo Receiver | 0.375 / 0.375 |
| Receiver Pan=-1，静音发送 | 0.25 / 0 |
| Receiver Pan=-0.5，静音发送 | 0.25 / 0.125 |
| Receiver Pan=0.5，静音发送 | 0.125 / 0.25 |
| Receiver Pan=1，静音发送 | 0 / 0.25 |

`D_PANLAW=-1`（继承工程），工程 `PANLAW=0`。复现脚本：[probe-routing.lua](../../../scripts/reaper_parity/probe-routing.lua)。在新的临时目录准备 source.wav/receiver.wav（上述常量素材），设置 AAADAW_REAPER_PROBE_DIR；只在新空工程、隔离配置运行。脚本拒绝非空工程，输出实际渲染文件与 reference.txt；不要覆盖用户工程或配置。

实现：Project/DawAction 允许普通轨道接收单输出，保存、快照、撤销一致。渲染依赖顺序与轨道显示顺序无关；接收轨道 fader、mute、solo 生效。Solo 在拓扑路由之前清除不可听的自身源缓冲，保留后续送入的上游内容。mono/stereo 每通道独立处理，未将收到的立体声折叠。

新工程默认 ZeroDbBalance，mono/stereo 使用相同 0 dB 线性 balance，接入静音路由不会改变自身素材增益。旧工程 schema17→18 增加 project_meta.pan_mode=0，保持 LegacyMonoStereo；新工程写 1。Live 调节与初始图使用同一策略。旧 Bus 仍保留固定 stereo 分类，TrackId、输出路径、FX 与媒体不改变。两种策略覆盖五个声像位置、连接/未连接静音路由；已有数值 fixtures 明确使用 legacy，新增默认策略测试独立覆盖。

迁移为单事务加列，失败不推进 user_version；关闭数据库后保存 v17 副本可回滚。旧程序不能读取 v18；恢复 v17 副本会失去迁移后的编辑，不提供伪无损降级。只读入口继续拒绝旧版本且不改文件，写入入口完成迁移。测试保留备份并校验字节未变。

未完成：并行 Send/Receive、Tap Point、Parent/Master 双路径、通道映射、MIDI Send、硬件路由、文件夹、可编辑 Pan Law/Width/Dual Pan、Routing Matrix/Wiring、PDC/反馈。此单输出能力不代表 REAPER 全量 Routing 通过。

验证结果：742 项 workspace 测试全部通过，0 skipped；Clippy warnings denied 通过，默认构建通过。聚焦审查确认新旧策略与路由兼容，已消除并发 Solo 切换时陈旧自身掩码的问题。
