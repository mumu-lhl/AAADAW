# 工程帧率与 H:M:S:F（验证进行中）

参照为 Linux REAPER 7.82 / Default 7 / 默认工程。自有空白临时工程运行 [探针](../../../scripts/reaper_parity/probe-frame-displays.lua)，通过 Alt+Enter → Video → Frame rate 逐项选择十种原厂预设；每次 OK 后记录 API 格式化与反解析结果及保存的自有空白工程数值。仅保留 [数值 TSV](frame-displays-reference.tsv) 和 [预设事实](frame-settings-reference.json)，不提交 REAPER 窗口截图、主题、二进制或 RPP 原文件。

默认 30 fps；23.976 为 24000/1001，29.97DF/ND 为 30000/1001。DF 与 ND 分开保存。ND 小时最少两位；DF 实测小时不补到两位。显示向下取整帧，不以四舍五入改变未编辑的样本位置。DF 跳号按非十分钟边界跳过 00/01；本阶段只支持标准非负 H:M:S:F 输入，原厂宽松/非法/负数输入规则仍待核验。

Core `FrameRate` 精确比率与标准格式/解析、`ProjectSettings`、可撤销 `DawAction::SetFrameRate`、Snapshot/恢复已接入；音频/PPQ 时钟与媒体位置不因帧率改变而重采样。schema 27 独立 `project_timecode` 单行表存储稳定预设代码，旧工程默认 30 fps。迁移细节见 [数据变更](../../reaper-parity/domain-and-storage-changes.md)。

当前已通过十种预设 121 行数值的显示/反解析回归、Core 撤销/重做与 Snapshot、十种设置事务存储/只读 round trip、真实结构的旧 26 schema 扩展后数据保持和重复打开、旧只读文件字节不变、非法编码拒绝。保存失败事务回滚也已验证：帧率表插入触发器注入 ABORT 后，完整轨道/元数据状态保持迁移前快照。最终 audio-device workspace 848 passed / 0 skipped，默认 Clippy warnings denied、fmt 与 diff 检查通过。

Item Properties 的 Position/Length 在 H:M:S:F 下按工程设置转换；Take Start in source 仍为 Time，淡化长度仍为 Time；未编辑字段保留完整样本值。完整 Project Settings 的入口/分组/几何、视频和时间偏移设置尚未完成；新增 Core 帧率接口不表示这些 UI 已验收。视频编解码、SMPTE/MTC 同步及负/分数采样 Item 时钟仍待后续实施。

实际 AAADAW GUI：四种单位按原厂顺序提供（[菜单](item-fade-ui/properties-frames-menu.png)），默认 30 fps 的位置与长度 .5 秒显示 `00:00:00:15`（[显示](item-fade-ui/properties-frames-selected.png)）。Apply `00:00:01:15` / `00:00:00:07` / Time 源偏移 .125 秒后存储 72000/11200/6000 样本，schema 27、帧率代码 5（[应用](item-fade-ui/properties-frames-applied.png)）。[一次 Undo](item-fade-ui/properties-frames-undo.png) 恢复 24000/24000/12000，[Redo](item-fade-ui/properties-frames-redo.png) 恢复全部已应用字段。[重启](item-fade-ui/properties-frames-restart.png) 保持 H:M:S:F 配置和工程值。截图仅含 AAADAW 自有窗口。
