# 输入与行为对齐规范

## 原则与证据

目标是 Linux REAPER 7.82 默认配置。本文定义实现和调查方法，不预先声称具体快捷键或手势映射已实测。P0 将实际映射填入原子条目；不能以历史架构表或现有 AAADAW 行为替代。

同一 Action 从菜单、工具栏、Action List、快捷键、鼠标、宏或控制器进入后，共享可用状态、Toggle、参数校验和工程结果。纯窗口/偏好动作由应用层管理；工程修改通过 `Project`/`DawAction`，不能为对齐绕过领域接口。

## 输入分发

| 归属 | 要求 |
| --- | --- |
| 文本/数值输入与按键捕获 | 优先接收输入；完成、取消、失焦和冲突按参照处理 |
| 插件编辑器 | 插件自身消费与宿主全局穿透规则分别实测；窗口身份明确 |
| 当前编辑器/弹窗 | 使用自身 Action Section 和上下文；不能向其他窗口重复派发 |
| 主窗口 | 处理未被消费且适用的全局动作；焦点切换不丢失选择 |

“优先”是事件所有权约束，不是未经测量的固定映射。原厂允许的全局穿透必须逐项登记。记录 Repeat、左右 Modifier、键盘布局、字符/物理键、数字键盘与平台修饰键差异；不可静默拒绝解析无法表示的绑定。

## 状态模型

分别管理 Track Selection、Item Selection、Note/Point Selection、Time Selection、Loop Selection、Razor Selection、Edit Cursor、Play Cursor、键盘焦点与菜单目标。联动策略由默认配置实测决定，不能因内部共用一个字段强制合并。

Transport 状态至少区分 Stopped/Playing/Paused/Recording；循环、录音模式和播放开始点属于独立状态。对每种状态测试 Play/Pause/Stop/Seek、标尺/空白/Item 点击、Home/End、开关 Repeat、改变 Rate、输入 Arm、Punch 边界和停止录音。

现有 Stop-to-start 与 Restart-to-project-start 策略仅为待核验实现。按钮、快捷键与菜单不能出现三套不同结果；循环在引擎 sample clock 上执行，UI 只显示和发出意图。

## 鼠标上下文

P0 分别采集 Arrange 空白、标尺、Item 主体/边缘/Fade/Gain、Take/Lane/Comp、轨道名/控件、Envelope Point/Segment/Item、MIDI Note/Edge/CC/Keyboard、Docker/Tab、FX List 和 Routing Cell。

对每个上下文测试左/右/中键、双击、拖动和滚轮，以及 Ctrl/Alt/Shift/Super 的适用组合。记录命中优先级、鼠标指针、Threshold、Snap、临时忽略、自动滚动、越界、跨轨、修饰键中途变化及释放结果。

右键打开菜单与右键框选的识别必须以位置与手势区分；不要用点击事件提前提交会破坏多选的状态。未选对象与已在多选中的对象分别验证。

## 手势事务

1. 命中时读取稳定对象 ID、起始坐标、初始选择和领域快照。
2. 移动期间建立可取消预览，实时反馈不得直接破坏素材或重复写历史。
3. 松开时根据最终参数原子提交；没有有效变化不制造历史。
4. Esc、捕获丢失、窗口关闭或对象失效的结果按参照登记；需要回滚时恢复初始快照。
5. 实时参数/自动化写入使用有界控制路径；连续手势的历史合并与自动化数据所有权分别处理。

多选操作提交一组逻辑事务；任何对象失败时按参照结果和领域原子性处理，不能留下未说明的半完成状态。

## 控件与窗口

Fader/Knob 记录映射曲线、单位、拖动方向、修饰精调、双击重置、精确输入、滚轮、上下文菜单及作用对象。TCP/MCP 显示同一 Project/Track，不复制混音状态。

菜单支持鼠标、键盘切换、Submenu、Esc 与点击外部；弹窗分别记录 Modal/Modeless、焦点、Enter/Esc、Apply/OK/Cancel、重开。焦点回到原编辑器并保留工程和选择；插件 GUI 和 Docker 的关闭不得误关主窗口。

## 配置与兼容

默认映射只从固定参照生成；用户自定义单独存储，冲突反馈和恢复默认按实际行为实现。迁移旧 Keyboard/Action Macro/Audio/Plugin Path 配置，未知或失效条目给出可理解反馈并保留其他有效设置。

UI 调整、布局恢复和配置切换不改变工程；工程属性通过可撤销 Action 修改。配置与工程的保存边界由[领域文档](domain-and-storage-changes.md)统一定义。
