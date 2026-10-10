# 工程帧率与 H:M:S:F（验证进行中）

参照为 Linux REAPER 7.82 / Default 7 / 默认工程。自有空白临时工程运行 [探针](../../../scripts/reaper_parity/probe-frame-displays.lua)，通过 Alt+Enter → Video → Frame rate 逐项选择十种原厂预设；每次 OK 后记录 API 格式化与反解析结果及保存的自有空白工程数值。仅保留 [数值 TSV](frame-displays-reference.tsv) 和 [预设事实](frame-settings-reference.json)，不提交 REAPER 窗口截图、主题、二进制或 RPP 原文件。

默认 30 fps；23.976 为 24000/1001，29.97DF/ND 为 30000/1001。DF 与 ND 分开保存。ND 小时最少两位；DF 实测小时不补到两位。显示向下取整帧，不以四舍五入改变未编辑的样本位置。DF 跳号按非十分钟边界跳过 00/01；本阶段只支持标准非负 H:M:S:F 输入，原厂宽松/非法/负数输入规则仍待核验。

Core `FrameRate` 精确比率与标准格式/解析、`ProjectSettings`、可撤销 `DawAction::SetFrameRate`、Snapshot/恢复已接入；音频/PPQ 时钟与媒体位置不因帧率改变而重采样。schema 27 独立 `project_timecode` 单行表存储稳定预设代码，旧工程默认 30 fps。迁移细节见 [数据变更](../../reaper-parity/domain-and-storage-changes.md)。

当前已通过十种预设 121 行数值的显示/反解析回归、Core 撤销/重做与 Snapshot、十种设置事务存储/只读 round trip、真实结构的旧 26 schema 扩展后数据保持和重复打开、旧只读文件字节不变、非法编码拒绝。保存失败事务回滚也已验证：帧率表插入触发器注入 ABORT 后，完整轨道/元数据状态保持迁移前快照。最终 audio-device workspace 848 passed / 0 skipped，默认 Clippy warnings denied、fmt 与 diff 检查通过。

Item Properties 的 Position/Length 在 H:M:S:F 下按工程设置转换；Take Start in source 仍为 Time，淡化长度仍为 Time；未编辑字段保留完整样本值。完整 Project Settings 的入口/分组/几何、视频和时间偏移设置尚未完成；新增 Core 帧率接口不表示这些 UI 已验收。视频编解码、SMPTE/MTC 同步及负/分数采样 Item 时钟仍待后续实施。

实际 AAADAW GUI：四种单位按原厂顺序提供（[菜单](item-fade-ui/properties-frames-menu.png)），默认 30 fps 的位置与长度 .5 秒显示 `00:00:00:15`（[显示](item-fade-ui/properties-frames-selected.png)）。Apply `00:00:01:15` / `00:00:00:07` / Time 源偏移 .125 秒后存储 72000/11200/6000 样本，schema 27、帧率代码 5（[应用](item-fade-ui/properties-frames-applied.png)）。[一次 Undo](item-fade-ui/properties-frames-undo.png) 恢复 24000/24000/12000，[Redo](item-fade-ui/properties-frames-redo.png) 恢复全部已应用字段。[重启](item-fade-ui/properties-frames-restart.png) 保持 H:M:S:F 配置和工程值。截图仅含 AAADAW 自有窗口。

## 帧率变化与已打开属性窗口的联动

[自有采样级探针](../../../scripts/reaper_parity/probe-item-timecode-settings.lua) 使用 48 kHz 自有媒体，初始位置/长度/源起点分别 16001/12001/6001 样本。初始化后打开 H:M:S:F 属性，只改 C=.25 并 Apply，三项精度保持；工程帧率 30→25 后，旧文本 `00:00:00:10` / `00:00:00:07` 保持且 Apply 仍禁用，工程媒体位置不立即变化。随后只改 C=.75 并有效 Apply，旧帧文本按 25 fps 解释，位置/长度变为 19200/13440，源起点仍 6001 样本。最后工程帧率 25→24，干净属性直接 OK，媒体仍保持前值。[数值记录](item-timecode-settings-reference.tsv) 保存完整变化序列。

实现为草稿记录显示时帧率：单位刷新/重开时更新；外部帧率修改不主动丢弃草稿。只有 H:M:S:F 草稿有实际编辑且帧率变化时，在提交前重新解析位置/长度两项，使用既有一次事务与播放守卫；干净 OK 不重解析。相同帧率下，未编辑字段仍保留精度。完整工程设置 UI 尚待推进，此联动规则先由领域设置变更回归验证。

联动修正最终验证：849 passed / 0 skipped；默认 workspace Clippy warnings denied、fmt 与 diff 检查通过。

## Project Settings 帧率入口（GUI 验证进行中）

新增 File → Project settings... / 动作列表 / 默认 Alt+Enter（实机 action 40021）入口，显式用户快捷键优先。窗口当前只提供已实现的 Video Frame rate 草稿与 OK/Cancel；选项修改不会即时修改工程，OK 通过一次 DawAction 提交，Cancel/Escape 丢弃草稿。帧率菜单打开时，Escape 重置该控件状态并只收起菜单；随后 Escape 才取消窗口，其他控件捕获的 Escape 仍先交给控件；忙碌和过期工程草稿不能写入工程，重复打开聚焦已有窗口。外部帧率变更保持已打开 Item Properties 的原文本，按上述实测规则提交。

窗口采用已测原厂 606×535 初始尺寸以容纳十项帧率菜单，但不表示全页、Project Settings/Media/Advanced/Notes/Video 全控件或 Save as default project settings 已实现。入口和控件仍等待下列真实 GUI 验证；完整页继续按矩阵推进。

本次验证：853 passed / 0 skipped；默认 workspace Clippy warnings denied、fmt/diff 通过。真实 X11 窗口已验证 Alt+Enter、十项菜单、第一次 Escape 收起菜单、第二次 Escape 取消；OK 选择 25 fps 后，活动恢复工程 schema 27 帧率编码为 2，一次 Ctrl+Z 恢复 30 fps 编码 5，Ctrl+Shift+Z 恢复编码 2。完整键盘菜单导航、各页布局与全部设置仍待实现。
