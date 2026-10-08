# AAADAW UI/UX Design Contract

本文是 AAADAW 桌面 UI 的产品设计 contract。以后新增或重构 UI 时，按本文决定信息位置、空间关系、交互和视觉优先级；若具体实现与本文冲突，应先修正设计或记录有理由的例外，不要把现有 widget 的限制当作产品要求。

本文与其他项目文档的职责如下：`DESIGN.md` 约束 UI/UX；[`ROADMAP.md`](ROADMAP.md) 决定功能范围和开发顺序；[系统架构文档](docs/design/AAADAW_System_Architecture_Design.md) 决定技术、领域和实时安全边界。架构文档中的高级功能矩阵不自动成为当前版本的交付要求。UI 中的工程状态仍必须通过 `aaadaw-core` 的公开 `Project` 接口和 `DawAction` 修改。

## Product Direction

AAADAW 是一款 **REAPER-inspired professional desktop DAW**：桌面优先、信息密集、功能优先、紧凑、高效，适合键盘和鼠标，也为高级用户保留较高的可配置性。Arrange View 是主窗口的固定工作区域；轨道控制与时间线协作；Mixer 完成后可作为独立且与轨道状态一致的主要工作区；Transport 始终清楚可访问。Media Browser 是可停靠的辅助面板，不是全屏工作区；工程管理和 Action Search 通过菜单与命令入口完成，不创建 Project 工具页。

REAPER 是产品工作流和信息架构的主要参照。继承可验证的工作方式，不追求逐像素相似，也不复刻其视觉主题。

以下不是目标：

- Web dashboard、营销落地页或卡片式管理台外观。
- oversized controls、过多的大卡片或圆角矩形。
- 大片空白边距、为了截图装饰而压缩工作内容。
- mobile-first 布局。桌面窗口可以缩放，但桌面键鼠工作流优先。
- 为了“看起来像完整 DAW”而提前展示尚未支持的功能。

## Main Window Architecture

主窗口围绕一个正在编辑的工程组织。全局命令在窗口边缘，空间编辑留给中央 Arrange View。主窗口不使用 Arrangement、Media、Project 这类子页面标签；辅助工作区以可停靠或隐藏的面板提供。

| 区域 | 职责与布局原则 |
| --- | --- |
| Menu bar | 提供 File、Edit、View、Insert、Item、Track、Actions 等按领域分组的全局命令。菜单是可靠入口，不承载大块常驻内容。 |
| Main toolbar | 放置少量高频命令和有明确开/关状态的模式，如 Snap、Grid、工具模式。图标需有 tooltip 和可辨识的 active 状态；用户可配置或隐藏。 |
| Track Control Panel (TCP) | 位于 Arrange View 左侧。每个轨道一行，显示身份和常用混音/录音控制；轨道顺序、选择、折叠和垂直滚动与 Arrange 行一致。 |
| Ruler / timeline | 位于 Arrange View 顶部，横向刻度与媒体区域使用同一时间坐标和水平滚动。显示当前项目时间基准、网格、标记和选区边界。 |
| Arrange area | 主编辑画布。按轨道行显示 Audio/MIDI Items 与可见的自动化；与 TCP 行高、顺序、选择和滚动同步。 |
| Transport | 固定在主窗口底部或用户选择的停靠位置。播放/停止、录音状态和当前时间始终可见；不得因切换工作区而消失。 |
| Mixer Control Panel (MCP) | 与 Arrange 并列的主工作区，可显示、隐藏、停靠或浮动。每个 strip 控制与 TCP 表示同一个 track。 |
| Dockable panels | FX browser、MIDI editor、media browser、routing/track list 等辅助工具可停靠、浮动、resize、hide/show。Media Browser 只能作为辅助面板；工程文件操作在 File 菜单，Action Search 在 Actions 菜单。面板不应夺走 Arrange 的默认主次。 |

Arrange 与 TCP 之间必须有可拖动分隔线；两侧共用轨道顺序和垂直滚动。主工具栏和辅助面板不应重复展示同一工程状态。工程名、dirty/saved、后台工作状态应在紧凑状态区可查。

## Arrangement View

Arrange View 是音频和 MIDI 的共同空间。列表、属性对话框和数值输入可补充精确编辑，但不能替代按时间和轨道排列的可视编辑区。

### Track rows and media items

- 每个 track row 与 TCP 中同一轨道一一对应；改变轨道顺序、行高或折叠状态时两者一起更新。
- Audio Item 和 MIDI Item 按起点与长度绘制在时间坐标上；不同类型可以有自己的内部绘制，但选择、移动、复制和边界处理遵循一致的外层模型。
- Insert → MIDI Item 放在当前选中的非 Bus 轨道上，起点始终是 Edit cursor；有时间选区时采用选区长度，否则创建四拍长度。
- 波形、MIDI 音符预览、item 名称和状态用于快速识别内容；装饰图形不得遮挡边界、选中状态或时间位置。没有媒体时，空白轨道仍可通过时间标尺定位。
- Item 是非破坏性工程对象。拖动或删除 item 改变工程 placement；对源文件的操作必须明确区分并按媒体工作流处理。
- 多个 Item 同时选中时，移动、复制、删除、对齐等操作对整组选区执行；目标 Item 和原有选择的反馈必须清楚。

### Ruler, cursor, selection and loop

- 时间标尺显示与工程时基一致的刻度。Bars/beats 是音乐编辑的主要显示方式；时间、samples 等精确单位可切换或在属性/Transport 中读取。Tempo 与拍号变化要在可见范围内可辨识。
- Edit cursor 是明确的细竖线；定位光标不能误选 Item。播放位置可与编辑光标区分，并在播放过程中保持可见。
- Item selection、track selection、时间选区和键盘焦点是不同状态，不能用同一种高亮混为一谈。选中内容需有稳定、低噪声的视觉标识。
- Time selection 表示一段编辑/处理范围；loop selection 表示循环播放范围。默认视觉上区分两者。允许用户选择将二者联动，但联动状态必须能观察和更改。
- 选区起止边界应能拖动；精确范围可通过 Transport、属性面板或键盘输入修改。Snap 是否开启和当前网格间距必须容易发现。Snap 开关与网格间距是两个独立控件；默认间距为 1/16，用户可选择常用音符时值以及附点、三连音细分。Item 移动和时间选区使用同一当前网格。

### Automation and envelopes

- Envelope 可显示在所属轨道或独立 lane 中，并标明被控制的参数。曲线、控制点和直线段清楚区分，点的选择和拖动有可见反馈。
- Envelope lane 可逐轨显示/隐藏、调整高度；显示 envelope 不得让普通 Item 选择和拖动变得含糊。
- FX 参数自动化在插件或轨道上下文中可找到；自动化数据在主时间线上仍可检查和编辑。

### Scrolling and zooming

- 水平导航控制时间范围，垂直导航控制轨道范围；两种行为和快捷键可预测。
- 缩放默认围绕当前光标、鼠标位置或当前选区之一，且所用锚点一致。用户能快速缩放到选区、所选 Item 和整个工程。
- 播放自动滚动策略（光标跟随、连续滚动等）可配置；播放期间的定位不能让用户丢失工程上下文。
- 窄窗口和大工程使用视口裁剪/虚拟化。可见内容不能被任意固定数量上限静默截断。

## Track Control Panel

每条轨道的控制按固定语义组织。实际可见布局可因窗口宽度、轨道高度、录音状态和用户 layout 而变化，但同一控制在 TCP 与 MCP 的名称、状态、快捷键和动作结果必须一致。

| 优先级 | 控件 | 规则 |
| --- | --- | --- |
| 始终保留 | Track name / index | 名称可直接重命名；轨道身份在窄布局仍可辨认。 |
| 始终保留 | Mute / Solo | 是状态 toggle，active 状态清楚；组合选择时支持批量操作。 |
| 录音工作流 | Record arm / Input monitor | Arm 与 recording 状态视觉区分；监听默认关闭，只能在 armed 轨道录音期间显式启用，并进入该轨道的 mix/FX/master 路径。录音前监听仍待实现。 |
| 常用混音 | Volume | 支持可见数值及连续调节；细调、重置和精确输入可发现。 |
| 常用混音 | Pan | 支持居中状态和可读的左右数值；双击重置到中心。 |
| 可访问 | Routing / Sends | 显示是否存在连接，并能打开详细路由；后续可支持从轨道间拖动建立发送。 |
| 可访问 | FX | 能看出是否有 FX 及 bypass 状态；可以打开/管理 FX chain。 |
| 可读反馈 | Meter | 播放或监听时显示实时电平，peak/clipping 状态可辨识。 |
| 轨道组织 | Folder state | 文件夹轨可折叠；子轨层级以缩进或结构线表达，并和 Mixer 同步。 |

宽度不足时依次采用紧凑排布、缩短文字并提供 tooltip、将低频操作折叠进轨道上下文菜单或轨道属性面板。不得隐藏轨道名、Mute/Solo/Arm 状态或当前选择反馈。FX、routing、folder 和输入细节可折叠；Volume/Pan 仍应能快速访问。轨道高度缩小时先隐藏次要文字和辅助 controls，保留轨道身份与核心状态；recording 可显示必要监听控件。

## Mixer

Mixer 提供横向浏览和密集混音。Channel strip 的语义顺序固定，个别布局可以隐藏低频字段：

1. Track name/index 与文件夹归属。
2. FX inserts / bypass 状态及其快捷入口。
3. Sends 摘要与 Routing 入口。
4. Pan 控件。
5. Meter 与主 Volume fader（读数和推子相邻）。
6. Mute、Solo、Record arm 状态与操作。

Master strip 有稳定且易定位的位置。用户可以隐藏轨道类别、调整 strip 宽度、选择紧凑布局并横向滚动；选中轨道后可以让 Mixer 定位到对应 strip。

TCP 和 MCP 不是两个状态系统。它们都读取同一 Project/Track 状态，并把交互转换为同一组 App message 和 `DawAction`。在 TCP 改变 Volume、Mute、Solo、Arm、Pan、FX 或 Send 后，Mixer 应立即反映；反向操作也相同。Track selection、folder collapse 和 meter 语义也应同步。

## Transport

Transport 在所有主工作区保持清楚可访问。空间紧张时保留主控制和主要读数，将次要控制移入 Transport 右键菜单或可展开区域。

从左到右优先级：

1. Play、Pause、Stop、Record、Loop/Repeat。按钮状态按当前传输状态变化；Recording 使用高优先级红色状态，armed 另用独立状态。
2. 当前播放/编辑时间读数。默认显示 Bars.Beats 或项目主要时间单位；可切换到分钟/秒、samples 等精确格式。
3. Tempo 和 Time Signature；可编辑值需显示单位和当前工程状态。
4. 当前时间选区起点、终点或长度，以及 Loop 状态摘要。
5. 后端/设备状态、播放率等次要信息；不可用后端显示简洁、可操作的解释，不显示一排无效控件。

Stop 与 Pause 的结果必须不同且清楚：Pause 保持当前位置；Stop 返回本次播放开始位置，Restart 从工程开头开始播放。停止时点击时间标尺或轨道下方空白区会移动编辑光标，并设置下一次播放起点；暂停时点击只移动编辑光标，不改变暂停点；播放中点击暂不 seek。编辑光标和播放光标须保持视觉区分。当前 Stop 复位策略固定，后续再评估是否提供用户配置。Record 和 Loop 在暂不支持时明确标记为不可用/未来能力，不假装可操作。

## Panels / Docking

辅助面板可以：

- 在主窗口内 dock/undock，作为 tabbed panel 使用。
- 调整宽度或高度；用户可隐藏并从 View 菜单、命令搜索或快捷键重新打开。
- 根据任务浮动到独立窗口或显示于第二屏。
- 恢复上次位置/尺寸，并提供恢复默认布局的入口。

首批适用面板：Mixer、FX browser/chain、MIDI editor、Media browser、routing matrix、track list/manager、项目/动作搜索。面板可以独立演进，但工程状态只在 Project 中保存一份。停靠系统按需实现，不要求初版一次提供所有方向、标签行为或工作区快照。

Track 的 FX 按钮打开该轨道的 FX chain 编辑器。左侧按处理顺序列出已插入插件，每项可独立启用/旁路；右侧显示当前选中插件的界面。添加插件和删除所选插件是分开的操作；添加按钮打开插件选择窗口，列出设置中已扫描到的插件，选中后插入到当前链。底部按钮区提供添加、删除和关闭等链管理命令。TCP 上的 FX 状态应反映链是否为空以及是否存在旁路项。插件界面属于所选 FX chain，不为每个插件重复创建主工作区或独立设置页。

Settings 增加 CLAP Plugins 类别，允许配置多个插件搜索路径并查看最近一次扫描结果。扫描在后台运行；路径、扫描命令和插件列表保持在同一类别中。插件路径是本机偏好，不写入工程；工程保存轨道上的有序插件引用、启用状态和后续支持的插件状态。

纯配置和偏好设置使用由菜单命令打开的小型独立设置窗口，不占用 Arrange 或主窗口工作区。主菜单提供 Settings 入口；设置按主题分组并保持固定、紧凑的桌面布局，窗口可关闭后立即返回原工作区。快捷键映射属于 Settings，不放在 Project 工具页。设置窗口仅在用户主动打开时出现，不自动弹出或抢占其他应用的工作流。

Settings 窗口采用左侧类别栏和右侧设置面板。点击“Keyboard Shortcuts”显示完整快捷键列表，右侧按 Action 分类列出命令；每条 Action 都有按键捕获、清除当前绑定和恢复该 Action 默认绑定的控件。未来增加其他设置时沿用左侧选择类别、右侧显示完整设置内容的布局，不添加空白类别。

## Interaction Model

不同编辑面有各自的上下文，但基础语义必须稳定：

| 输入 | 一般原则 |
| --- | --- |
| Left click | 选择或激活目标；在 Arrange 空白处定位 edit cursor。点击控件执行其明确动作。 |
| Double click | 在名称/数值上进入直接编辑；在 Item 上打开主要编辑器或属性视图。不得让双击动作无法预期。 |
| Right click | 打开命中对象相关的上下文菜单。点击未选中的目标时，将其作为菜单目标；若目标已在多选中，保留多选。 |
| Drag | 只在明确的把手、边缘或 Item 主体开始拖动。开始后给出吸附/移动/缩放反馈；释放后提交一次 undoable action。 |
| Modifier + drag | 使用稳定的含义，例如 Ctrl/Cmd 拖动复制、Shift 暂时忽略 Snap、Item 内部拖动用于 slip edit。冲突时按控件语境区分并提供提示。 |
| Keyboard | 高频命令有默认快捷键；快捷键与菜单、工具栏、命令搜索共用同一个 Action 定义。文本输入、数值编辑和插件窗口拥有焦点时，不得误触发主窗口快捷键。 |
| Context menu | 提供与命中对象相关的常用命令；命令名称与菜单、Action 搜索一致，危险/破坏性操作在触发前清楚表达结果。 |
| Selection | Click 选择；Ctrl/Cmd click 增减选择；Shift click 选择范围。各编辑器明确说明跨轨道/跨 Item 选择与焦点关系。 |

修饰键的默认映射应遵循桌面音频编辑惯例，但形成 AAADAW 自己的一套一致映射，不要求逐键复制 REAPER。用户应能重映射高频操作；映射、菜单文字、toolbar button 和 Action search 不得各自实现不同命令。

快捷键设置以命令列表和当前绑定为主，不要求用户学习或输入快捷键语法。点击某命令的绑定控件后进入按键捕获状态；随后按下的组合键成为候选绑定，界面须显示正在录制、候选键、冲突或成功状态。用户可清除单条绑定、恢复全部默认值，重复绑定需给出明确冲突并保留原设置。文本输入与捕获控件聚焦时，不触发主窗口快捷键。

鼠标精调控件支持拖动、modifier 精细调整、双击重置和数值输入。所有长按/拖动手势都要保留键盘和菜单替代入口；操作不可因小热区或纯颜色提示而难以发现。

## Visual Language

- 使用 compact desktop controls；常用控件对齐，标签简洁，数值使用一致格式和单位。
- 以低对比背景承载工作内容；高饱和强调色只用于选择、模式开启、录音、告警等有意义状态。
- 用轻分隔线、面板边界和留白分组。减少无功能边框、层层嵌套卡片、渐变和大圆角。
- 建立一致的 spacing token：`spacing-xs`、`spacing-sm`、`spacing-md`、`spacing-lg`；`panel-padding`、`row-gap` 和 `section-gap` 从 token 取值，不在各 view 任意写 magic number。
- 建立控件和布局 token：`control-height-compact`、`control-height-standard`、`track-row-compact`、`mixer-strip-compact`。先按内容与缩放适配定相对关系，不在本文件强制全局像素值。
- Typography 分为工程/轨道身份、主要数值、辅助标签和状态提示四级。身份和电平/时间数值优先可读，辅助信息弱化但不能低于可读阈值。
- 使用清楚的 active、selected、armed、recording、bypassed、muted、soloed、clipping 状态。Meter 采用分段/渐变电平语义：正常电平绿色系、接近峰值黄色系、clip 红色；warning 使用琥珀色。颜色之外同时使用形状、标记、文字或状态灯表达含义。
- 当前工程、活动工作区、focus 和当前播放位置各自有一致且互不混淆的表现。
- 产品品牌、图标和控件主题需形成自己的视觉身份；仅用文本、开源自制图标或项目拥有/许可清晰的资源。

## Responsive / Dense Layout Behavior

桌面仍是主工作模式。小屏设备使用独立的紧凑触控 profile；不得把桌面窗口简单缩小后当作手机界面。

- 主窗口优先保障 Arrange 的可用时间宽度，其次保障 TCP 的轨道身份和核心控制；辅助面板可自动变为 tabs 或隐藏，Transport 保持可见。
- TCP 和 Mixer strip 提供 compact/normal/expanded 密度。compact 状态隐藏重复文字与低频控制，依然保留 name、selection、M/S/Arm、Meter 和主要增益入口。
- 视口宽度不足时，长名称截断并显示 tooltip；时间读数切换较短格式；按钮不应互相挤压或越界。
- track row 变矮时优先保留轨道标签、选择、Mute/Solo/Arm 和正在录制状态；FX/sends 细节可移入菜单。
- Mixer strip 变窄时保留可操作的 fader、meter 和 mute/solo 状态；FX 与 Sends 显示为短标签/状态标记，详情由面板打开。
- resize 不改变工程数据、track order、选择或 Transport 状态。分隔线需有可发现的拖动反馈，并支持恢复默认布局。
- 开发时至少检查默认窗口、窄窗口和高 DPI/系统缩放场景；不以单一桌面分辨率调 CSS/布局。

### Android 小屏触控 profile

- 逻辑宽度低于 720 dp 时使用单栏 shell：菜单横向滚动，Arrange/Mixer 作为明确的工作区切换，Transport 固定在底部。
- 手机 Arrange 以时间线为主画布。轨道选择横向滚动；当前轨道的 Mute/Solo/Arm、音量和声像放在时间线前方，不要求用户维护 TCP 与时间区的并排分隔器。选中 Item 后，同一区域切换为 Item Inspector。
- 手机 Mixer 保留横向 channel strip 浏览。轨道名称、Meter、主推子及 Mute/Solo/Arm 状态始终可见；推子和状态按钮使用触控尺寸。
- 高频按钮的最小可触目标为 48 dp；M/S/R 不能只靠颜色表达状态。拖动控件仍提供可访问的精确输入或菜单操作。
- 手机 Transport 只常驻 Play/Pause、Stop、Record、BPM 和当前定位。设备详情、Seek、MIDI Panic 与诊断放入可展开菜单，不挤占时间线宽度。
- Media、设置、插件和时间地图使用单面板导航；返回主编辑器后恢复工程、选中轨道、光标和播放状态。
- Android 触控手势应调用现有 Action 和 Timeline 消息。虚拟修饰键仅显示已有且可执行的编辑模式；不得把尚未实现的模式做成可点击假控件。
- 屏幕旋转、系统栏 inset、软键盘出现与前后台切换不得丢失工程状态或触发意外全局快捷键。

## Iced Widget Strategy

Shell、菜单、按钮、文本输入、对话框、列表、状态提示等优先使用 Iced 0.14 的 retained widgets。DAW 专用交互由可复用的 Iced 组件承载，视情况实现自定义 Widget 或 `iced::widget::shader`/wgpu 视口：

- Arrange 时间标尺、轨道背景、Item hit-testing、选择框和播放光标。
- Audio waveform 和 MIDI note preview；后续钢琴卷帘、CC lanes、自动化控制点。
- 小尺寸、高频的轨道推子/声像/电平表，以及可 resize 的面板分隔条。

自定义绘制只承担需要的时间/信号可视化和密集命中测试；状态与命令仍由 Iced message/update 流驱动。Widget 不持有第二份可写 Project，不绕过 App、`Project.apply(DawAction)`、撤销/重做和校验。时间线渲染须有视口范围和数据缓存，不随工程全量重算。

## UI Acceptance Criteria

每个重要 UI ticket 完成前必须：

1. 按仓库使用的 Rust/Iced 版本编译目标应用；行为测试使用仓库约定的 `cargo xtest`，不以单个 crate 测试的通过代替项目要求。
2. 运行真实应用，在目标窗口查看实际布局和关键状态；不能仅凭代码、DOM/Widget 树或单元测试判断视觉完成。
3. 对照本文逐项检查信息层级、对齐、密度、分隔、resize 行为、焦点/选择/hover/pressed/active/disabled 状态和错误/后台状态。
4. 实际操作新增交互：鼠标、相关修饰键、键盘导航、上下文菜单、精确输入和撤销/重做；验证文本输入不会触发意外全局快捷键。
5. 检查 TCP 与 Mixer 的同一 track 行为和状态同步；涉及工程变更的 UI 必须走公开 `Project` 接口和 `DawAction`。
6. 检查默认、窄窗口和至少一种非默认状态（如选中/静音/armed/播放）；发现明显溢出、重叠、难以读数或控件抢空间时先修复再完成。
7. 如本机图形会话、音频后端或外设不可用，在交付说明中指出实际未能检查的情形，不声称通过视觉/设备验收。

## REAPER Reference Policy

**允许参考**：信息架构、主工作区关系、TCP/Mixer 的双入口状态一致、时间标尺和项目对象的空间编辑、Action/快捷键/上下文菜单共用命令、面板可配置，以及高密度桌面工作流。此处的“为什么”是从 REAPER 手册记录的实际布局和交互推导：例如 TCP 宽度和轨道高度可调、窄布局隐藏部分次要控件、Mixer 复用轨道控制、不同上下文提供菜单/Action/快捷键、Transport 和 Docker 可停靠或隐藏。我们的结论是：这些做法保留工作画布、缩小状态差异并让熟练用户减少切页；这是设计推论，不是对 REAPER 内部设计意图的事实断言。

**禁止复制**：REAPER/Cockos 专有图标、logo/branding、插画、截图作为产品素材、主题资源、像素级布局，以及其未授权的图片、皮肤或其他受版权保护的视觉资产。参考图片仅用于内部理解并保留原始来源链接；不得打包进 AAADAW 或将参考本身当成 UI 资源。

**调查来源（访问日期：2026-10-01）**

- [REAPER 官方 User Guide 页面](https://www.reaper.fm/userguide.php)：页面列出当前版本 v7.81 手册。
- [REAPER User Guide v7.81（官方 HTML 手册）](https://dlx.reaper.fm/userguide/html/)：重点阅读 1.9–1.14（选择/命令、主窗口、TCP、Arrange/Mixer）、2.3、2.17–2.18、2.21–2.27（Transport、时间/循环选区、路由、dockers）、5.2、5.9、5.19–5.20（轨道控件、layouts、toolbar）、6.5–6.7（FX）、第 7 章与 7.35（Items 和 mouse modifiers）、第 11 章（Mixer）、第 13 章（MIDI editor）及第 15 章（菜单、Action 和快捷键）。该手册版权归其作者，本文仅总结交互事实，不复制正文或图片。
- [Cockos REAPER 产品与自定义能力介绍](https://www.reaper.fm/about.php)：用于核对 dock/hide、layouts、theme、toolbar 和 Action 配置能力。
- 官方公开界面参考：[TCP](https://www.reaper.fm/v7img/tcp.jpg)、[Routing](https://www.reaper.fm/v7img/routing.jpg)、[FX/EQ](https://www.reaper.fm/v7img/eq.jpg)、[Theme](https://www.reaper.fm/v5img/320_theme.jpg)。这些图片只作为 REAPER 产品资料的链接，不复制到本项目。

## Current UI Audit

本审计于 2026-10-08 对照当前 `main`、`crates/aaadaw/src/app/view/`、`timeline.rs` 和 #216 重新核对。主工作区已有可编辑的空间编排、音频/MIDI 边界操作、Snap、自动化和对象菜单；下表只记录仍可从代码确认的缺口，不重复列出已完成的旧审计项。真实设备、窄窗口、高 DPI 和端到端操作仍需单独验收。

| 严重度 | 维度 | 当前问题 | 后续影响 / 处理 |
| --- | --- | --- | --- |
| P1 — 时间线导航不完整 | Interaction / transport | 视口已有滚轮缩放、中键拖动缩放/平移、Snap、片段选择/拖动/修边/切分和自动化编辑；还没有循环播放、播放光标跟随，以及 Fit Project、Fit Selection、Fit Selected Items。编辑光标与播放起点也需要保持清晰一致。 | 补齐常用导航命令并统一缩放锚点；时间选区与循环状态须可区分。验证播放、暂停、停止和点击定位的起点语义。 |
| P1 — TCP 密度和 Mixer 工作流 | Layout / mixing | TCP 已有轨道选择、名称、Volume、Pan、Mute/Solo/Arm、输入监听、左右电平表、FX 和输出选择；固定行高与紧凑控件在窄布局下仍有裁剪风险。Mixer 已存在，但通道固定约 220px，采用横向推子和电平条，Master 随横向滚动且没有输出音量推子。 | 先解决 TCP 行高/密度与可读性；Mixer 使用纵向推子及相邻电平表，保持 Master 可见，并让两个视图操作同一轨道状态和撤销历史。 |
| P1 — 工程身份与切换保护 | Window / project workflow | 主菜单显示工程名和未保存标记，但操作系统主窗口标题仍为 `AAADAW`。New/Open/Close 已提供保存、放弃或取消选择，并在保存成功后继续原操作。 | 让操作系统窗口标题同步工程名/dirty 状态；保留已有切换确认语义，保存失败或取消选择器时保持当前工程。 |
| P1 — Transport 的缺失操作 | Transport / recovery | Transport 已有 Play/Pause、Stop、Record、Restart、seek、BPM/拍号编辑、播放准备反馈和后端详情；Loop 仍显示 unavailable，时间线导航缺少跟随/适配入口。Linux 后端状态和路由诊断正在 #271 / PR #273 实施，尚未完成验收。 | 保留现有窄窗口布局；实现循环与导航前先定义状态语义。设备/路由诊断不得占用音频回调，也不能暗示已连接即有声音。 |
| P2 — Inspector 与素材发现 | Panel / editing | Inspector 选中片段时高度从 52px 增至 156px，并为 MIDI 音符显示大量按钮；目前没有折叠/resize。Media Browser 支持导入、扫描、打包和重链接，但没有通用目录浏览、试听及拖入编排。 | Inspector 保持简洁并允许折叠/调整高度；素材浏览、试听和拖放应复用现有媒体导入及后台任务能力。 |
| P2 — 国际化和视觉可读性 | Localization / accessibility | 界面文本仍以内嵌英文为主，没有项目级翻译资源或语言设置。Dark 主题固定，多个高频状态依赖缩写/颜色，字号、读数格式和高 DPI 表现尚未统一验收。 | 引入可由翻译平台维护的资源格式、中文/英文和自动语言选择；再统一焦点、选择、录音、旁路等状态提示及可读数值。 |
| P2 — 渲染与大型工程验收 | Performance / offline workflow | 渲染窗口已独立，但范围、输出采样率和尾音策略仍有限。长工程的轨道布局、渲染和滚动尚无基准；当前实现路径存在按全工程构建部分视图数据的开销，但未测得实际卡顿。 | 用可复现的大工程基准先测量，再决定虚拟化和缓存；补充工程/选区渲染范围、时长估计和尾音控制。避免无数据依据的性能重构。 |
| P2 — 真实工作流验收 | Verification | 自动测试覆盖项目模型和多种编辑行为，但还没有记录完整的键鼠、音频编排、MIDI 创作、录音、bus 混音、范围渲染以及保存重开的端到端验收。 | 建立可重复的本地验收清单；自动测试继续覆盖确定性语义，真实音频设备、高 DPI、多平台和大工程单独注明环境与结果。 |

## UI Modernization Roadmap

按依赖关系逐步改进，不做全 UI 一次性重写。此 roadmap 是设计建议，不代表扩大 `ROADMAP.md` 的功能范围；已实现的基础能力不再作为未来工作重复排期。

1. **[x] 建立空间 Arrangement 基线**：已有时间视口、轨道/item 几何、选择、缩放/平移、Snap、音频/MIDI 移动与修边、切分、波形/音符预览、音量/FX 自动化和对象上下文菜单。后续针对已有能力修复具体缺陷，不再重建主工作区。
2. **重排 TCP 并完善 Mixer**：解决 TCP 行高与左右电平表可读性；Mixer 改为纵向推子和紧凑通道，固定可见 Master，保持 TCP/Mixer 控制和 Undo/Redo 同源。
3. **补齐时间线导航与工程切换**：实现 Loop、Follow Playhead、Fit Project/Selection/Selected Items 和统一缩放锚点；让窗口标题与工程名一致。New/Open/Close 的保存、放弃、取消流程已实现。
4. **改进 Inspector 与 Media Browser**：允许 Inspector 折叠和 resize，减少全量音符按钮；为媒体浏览增加目录、试听和拖入入口，同时保留导入、扫描、打包、重链接与后台取消。
5. **完成音频与渲染工作流**：结束 #271 的 Linux 连接/路由诊断；补渲染范围、采样率、尾音控制，以及真实设备上的录音/路由验收。
6. **国际化、性能和视觉系统**：以翻译资源提供中文/英文及自动选择；先用长工程和高 DPI 验收建立性能/可读性基线，再优化可见性和布局恢复。

## Current Implementation Worth Keeping

- `App`、`Message`、workspace 和 view 模块已拆分，适合继续按职责深化；view 读取状态并发出 message。
- Project 修改走 `Project`/`DawAction`，撤销/重做和播放时编辑保护有可复用的产品语义。
- Native file picker、后台导入/扫描/pack/relink、进度和取消状态都适合作为 Media panel 的基础。
- Audio Item 的 sample-clock 精确定位、MIDI 的 tick/音符编辑、Audio/MIDI 相关 Action 不因列表视图退役而丢弃；可保留为 Inspector 和键盘命令。
- Transport 已占据稳定底部位置；可沿用作为常驻容器，并替换其 placeholder/简化控件。
- Transport 现提供 edit-cursor BPM/拍号读数和独立 Tempo/Meter Map 窗口；支持位置、BPM、拍号、曲线和原子 Undo/Redo。窗口的原生视觉/键鼠验收仍需图形会话运行。
- 可选 JACK 的播放准备、seek refill、播放状态和异步任务边界保持在 UI 重构之外。
- 独立 Media workspace 对工程级维护有价值；状态/进度应在 dock 后仍可访问。

## Rework When Replacing

- 不要再以列表替换主 Arrangement；继续深化现有时间视口。仅在有测量证据时增加轨道/item 虚拟化或缓存。
- 优先改善已有 TCP 和 Mixer 控件的行高、密度、可读性与共享状态；避免重新实现一套不经过 `Project`/`DawAction` 的混音状态。
- Actions、主菜单、快捷键和对象上下文菜单已有共享命令基础；新增入口应复用注册表，不另建平行命令系统。
- Inspector 当前由选中状态强制改变固定高度；扩展前先提供折叠和用户可调高度，避免继续堆叠低频控件。
- 主菜单已显示工程名和未保存标记；New/Open/Close 已提供保存/放弃/取消流程，主窗口操作系统标题现随工程名和未保存状态更新。
- Item/note 主视图没有旧的固定 200 项显示上限；针对真实大型工程数据先做基准，再选择视口裁剪、索引或缓存策略。
