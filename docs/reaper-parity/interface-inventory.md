# 界面调查清单

本文是 P0 的分类索引，**不是已完成的全量窗口枚举**。下表对应矩阵的 `category` 行；P0 必须按固定参照实际入口展开为 `atomic` 行，并补齐新发现。

## 分类与调查入口

| ID | 界面 / 行为类别 | P0 调查内容 | 阶段 |
| --- | --- | --- | --- |
| UI-001 | 主菜单 | File/Edit/View/Insert/Item/Track/Options/Actions/Help、子菜单、禁用、勾选、助记键、导航 | P1/P2 |
| UI-002 | 工具栏 | 默认/浮动工具栏、右键、按钮状态、编辑器、自定义图标/动作、位置 | P1/P2/P11 |
| UI-003 | Docker 与窗口 | 四向位置、Tabs、浮动、置顶、分隔、关闭恢复、多屏、Screensets | P2 |
| UI-004 | TCP | 各高度布局、Name/Index、Arm/Input/Monitor、FX/Route/Trim、Gain/Pan/Width、Phase、Meter、文件夹 | P2/P3/P6 |
| UI-005 | MCP/Master | Layout、Fader、Meter/Peak、FX/Sends、Master、滚动、显示过滤、选择同步 | P2/P3 |
| UI-006 | Arrange 与标尺 | Item/Track/Time/Loop 选择、鼠标修饰键、Snap/Grid、缩放、滚动、播放跟随、标尺单位 | P1/P4 |
| UI-007 | Transport | 播放/停止/暂停/录音/Repeat、时间显示、Rate、Tempo、拍号、选区、右键、Big Clock | P6 |
| UI-008 | 工程与模板 | New/Open/Save/Close、项目标签、Settings 全页、模板、Subproject、Notes、Recovery | P10 |
| UI-009 | Track/Item 属性 | 重命名、颜色、轨道设置、Item/Take/Source 属性、精确值和批量编辑 | P3/P4/P5 |
| UI-010 | Item 编辑 | Move/Copy/Trim/Extend/Slip、Fade/Crossfade、Gain、Split/Heal/Glue/Reverse、Source Loop | P4 |
| UI-011 | Take/Lane/Comp | 录音 Take、Fixed Lanes、Comp Areas、激活/多 Take、评级、Marker、Explode/Implode | P5/P6 |
| UI-012 | 高级与谱编辑 | Razor、Ripple、Grouping、Lock、Stretch、Pitch、瞬态、动态分割、谱图/谱编辑/Sample 编辑 | P5 |
| UI-013 | Tempo/Marker/Region | 标尺 Lane、Tempo/Meter、属性、管理窗口、Region 编辑、Timebase、负时间偏移 | P4/P10 |
| UI-014 | 路由 | Track I/O、Send/Receive、Hardware、Matrix、Wiring、Channel/MIDI Bus、反馈、PDC | P3 |
| UI-015 | Pan/Group/VCA | Pan Law/Modes、Width/Dual Pan、Phase、Grouping Matrix/Manager、VCA | P3/P8 |
| UI-016 | 音频设备与录音 | 后端、设备/通道、别名、缓冲/时钟、Monitor、模式、Punch/Loop、录音结束、恢复 | P6 |
| UI-017 | 节拍器与同步 | Metronome/Count-in/Pre-roll、路由、Scrub/Jog、外部 LTC/MTC/MIDI Clock 等适用入口 | P6/P11 |
| UI-018 | MIDI Piano Roll | 菜单/工具栏、音符/键盘、工具、试听、颜色、选择、过滤、缩放、局部动作 | P7 |
| UI-019 | MIDI 事件与处理 | CC Lane、Velocity/Pitch、Program/Bank、Pressure/SysEx、事件列表、属性、量化/Humanize | P7 |
| UI-020 | MIDI 其他编辑器 | Inline、Pool、多 Item、Drum、Virtual Keyboard、导入导出、Notation | P7 |
| UI-021 | 包络与自动化 | Envelope 选择/属性/曲线、Track/Take/FX、模式、写入、Automation Item/Pool、Trim | P8 |
| UI-022 | 调制与 Learn | LFO/Audio Control/Parameter Link、MIDI/OSC Learn、冲突及控制优先级 | P8/P11 |
| UI-023 | FX Browser/Chain | 格式/类别/收藏/搜索、插入删除排序复制、Presets、浮动、Generic UI、参数与焦点 | P9 |
| UI-024 | FX 包装与位置 | Track/Input/Take/Master/Monitor、Pin、Wet/Dry/Delta、Oversampling、Offline/Bypass | P9 |
| UI-025 | 原厂处理与特殊 FX | 内置 DSP、JSFX、空间声像、外部 Insert、网络 FX、插件故障/缺失 | P9/P11 |
| UI-026 | Media Explorer | 目录/数据库、搜索、元数据、波形、试听、Tempo/Rate、选区、拖入 | P10 |
| UI-027 | 工程管理 | Project Bay、Track Manager、Undo History、Notes、目录清理、Consolidate、冻结 | P10 |
| UI-028 | Render 设置 | Source/Bounds、Rate/Channels、格式/Tail、Wildcards、Presets、Metadata、Normalize/Limit | P10 |
| UI-029 | Render 管理 | Queue、Region Matrix、Stems、Online/Offline、Dry Run、统计、Batch Converter | P10 |
| UI-030 | Action List | Sections、搜索/运行、绑定、Custom Action、ReaScript、导入导出与管理 | P1/P11 |
| UI-031 | Preferences | 所有适用叶子页、类别导航、搜索、Apply/OK/Cancel、默认值与配置恢复 | P11 |
| UI-032 | Theme 与自定义 | Theme Adjuster/Tweaker、Layout、Screensets、菜单/工具栏编辑、Color Picker、语言 | P2/P11 |
| UI-033 | 辅助窗口 | Navigator、Big Clock、Performance Meter、资源/项目管理窗口、Help/About | P11 |
| UI-034 | 视频 | Video Window、媒体、Processor、参数/预设、脚本、编码/Render 和相关设置 | P10/P11 |
| UI-035 | 脚本与扩展入口 | ReaScript/JSFX Editor、运行、调试、Console、API 行为、资源路径 | P9/P11 |
| UI-036 | 外设与网络 | MIDI/OSC Mapping、Control Surfaces、双向反馈、Web Remote、网络能力和服务生命周期 | P11 |
| UI-037 | 系统与失败状态 | 文件选择器、保存保护、缺媒体/插件、设备掉线、磁盘满、取消、恢复、置顶/DPI | P2–P12 |
| UI-038 | 跨平台回归 | Windows/macOS 构建及适用功能、Android 触控/生命周期/工程互通 | P12 |

## 每个入口的展开规则

1. 记录准确窗口名和入口路径；一个窗口多个入口建立交叉引用，不能重复计数抬高覆盖率。
2. 枚举默认、Hover/Pressed/Selected/Disabled/Focused/Armed/Recording/Bypassed 等实际适用状态。
3. 每个控件记录几何、标签、单位、范围、默认值、Tooltip、右键、精调/重置、键盘路径和可访问反馈。
4. 每个鼠标上下文记录 Click/Double-click/Drag/Wheel、各修饰键及按下/释放中途变化。
5. 每个对话框记录打开条件、焦点、Modal/Modeless、Apply/OK/Cancel、持久化、关闭重开与异常。
6. 每个工程操作记录 Action/Undo、播放中的允许行为、保存重开、批量和失败的原子性。
7. 由支持功能反查所有 UI 入口，再由菜单/Action/Preferences 反查功能，双向查漏。

## 原子条目与封闭条件

示例 ID：`ARR-ITEM-MOVE-001`、`FX-CHAIN-COPY-001`、`PREF-AUDIO-DEVICE-001`。一个条目必须有可独立通过/失败的断言；只有“整个 Mixer 对齐”的描述不能作为原子条目。

每个类别登记所有适用原子条目后，记录调查证据与枚举日期。全部类别以及新发现类别调查结束，且主菜单、全部编辑器、Preferences、Action Sections、鼠标上下文、失败入口双向复查无遗漏，才可封闭分母。后续新发现继续纳入，不因封闭而拒绝修正。

## CSV 使用约定

初始矩阵包含 38 个分类索引及 16 个源码差距种子，全部为待调查；这不是全量原子清单，也不是功能缺失总数。种子已给出代码落点和初步断言，P0 实测后继续拆分或修订。

| 字段 | 含义 |
| --- | --- |
| `id / kind / parent_id` | 唯一 ID；`category` 或 `atomic`；原子条目所属分类 |
| `platform / reference_version` | 适用平台与固定参照版本；跨平台分开登记 |
| `surface / requirement / phase` | 界面、可观察要求、实施阶段；多个阶段以 `/` 分隔 |
| `status / blocked` | 调研/实现/验证状态；阻断独立为 `true/false`，原因记入差异 |
| `current_code` | 经源码确认的当前落点；没有实现可留空，不虚构位置 |
| `reference_evidence` | 固定参照证据 ID 与可访问位置 |
| `acceptance / verification_evidence` | 可判断通过/失败的断言，以及 AAADAW 验证结果和证据 |
| `difference` | 已知差距、算法/平台差异、阻断原因及必要说明 |

矩阵 ID 在后续提交、测试和证据中保持稳定；拆分时保留父项追溯。无适用性的条目需给出平台/扩展边界证据，不能仅因未实现就标为不适用。`passed` 行必须填参照及验证证据，类别行即使完成也不参与原子覆盖率。
