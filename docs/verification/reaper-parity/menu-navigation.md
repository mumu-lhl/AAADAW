# Main menu keyboard navigation

## REAPER 7.82 reference

在 Linux REAPER `7.82/linux-x86_64`、Default 7 和 Dummy Audio 下，用隔离 X11 输入向主窗口发送按键。F10 不打开菜单；F10 后的 Enter 仍执行默认 Play 动作。原厂 `Alt+F/E/V/I/O/A/H` 分别打开 File、Edit、View、Insert、Options、Actions 和 Help。重复按 `Alt+F` 时 File 菜单保持打开。打开 File 菜单后按 ArrowUp 会从顶部转到末项 Quit，但不会运行该命令；选择、传输状态和截图哈希见[菜单边界键 fixture](fixtures/menu-keyboard-boundary-native.tsv)。探测的 `Alt+T` 和 `Alt+M` 不打开菜单；Track 与 Item 菜单可从已打开菜单用左右方向导航到达。逐项结果见[原厂 TSV](fixtures/menu-navigation-native.tsv)，菜单截图分别为[File](fixtures/menu-file-native.png)、[Edit](fixtures/menu-edit-native.png)、[View](fixtures/menu-view-native.png)、[Insert](fixtures/menu-insert-native.png)、[Options](fixtures/menu-options-native.png)、[Actions](fixtures/menu-actions-native.png)和[Help](fixtures/menu-help-native.png)。

## AAADAW 当前实现与差异

主窗口按原厂已验证的 `Alt+F/E/V/I/O/A/H` 打开 File/Edit/View/Insert/Options/Actions/Help 菜单；快捷键只在 Iced 将按键标为未消费时触发。同一访问键会重新打开对应菜单，不会意外将它关掉。F10 不再打开菜单。主菜单栏和左右导航现有原厂九项顺序。REAPER 原厂 File 菜单的 `Close project` 使用 Ctrl+F4，Main Action List 40860 名称为 `Close current project tab`；AAADAW File 菜单现提供 `Close project`，Ctrl+F4 通过未保存工程 Save / Don't Save / Cancel 保护后关闭当前工程并回到空工程。原厂 File 菜单参考与 AAADAW 菜单、确认框截图/哈希见[Close project fixture](fixtures/menu-close-project-aaadaw.tsv)。AAADAW 目前只有单工程模型，不实现 REAPER 的多标签工程和 Close all projects。

REAPER Main Action 40022 `File: Save project as...` 默认 Ctrl+Alt+S。AAADAW File 菜单名称、快捷键和默认绑定现已对齐；该命令复用已有保存文件选择器。真实 AAADAW 菜单截图和 SHA-256 见[Save project as fixture](fixtures/menu-save-project-as-aaadaw.tsv)，定向回归确认默认组合映射为 SaveProjectAs，执行后进入保存选择器状态。Xvfb 中没有可见的原生文件选择器窗口供截图比对。

REAPER 原厂 File 末项为 `Quit`，Action List 名称 `File: Quit REAPER`，快捷键 Ctrl+Q（Action 40004）。AAADAW 现将 Quit 放在 File 菜单末项；ArrowUp 从首项循环到 Quit，Ctrl+Q 发起主窗口关闭并沿用 Save / Don't Save / Cancel 未保存工程保护。AAADAW 边界和实际退出确认截图、步骤、状态及哈希见[菜单 Quit fixture](fixtures/menu-keyboard-boundary-aaadaw.tsv)。File 其余原厂命令尚未齐全，因此菜单整体顺序仍不一致。Options 包含 Preferences...（Ctrl+P）和“Loop points linked to time selection”开关；后者显示当前状态勾选，执行后通过现有全局偏好保存路径持久化，不修改工程修订。Help 有“Action list and shortcuts...”入口和单实例 About AAADAW 窗口，`Ctrl+F1` 打开 About。

回归 `quit_matches_native_file_menu_and_uses_dirty_close_confirmation` 验证 Quit 为 File 菜单末项、短标签为 Quit、分隔线及默认 Ctrl+Q，并覆盖关闭确认取消和放弃继续。`options_menu_exposes_preferences_and_updates_the_loop_link_checkmark` 验证菜单命令、快捷键绑定可用、勾选态切换、全局偏好排队保存和工程 revision 不变；`help_menu_opens_the_action_list_and_a_single_about_window` 验证帮助入口、About 复用同一窗口、关闭清理和 Ctrl+F1；菜单键盘回归覆盖 Alt+O/Alt+H、九项左右顺序和启用命令首尾循环。nightly+Cranelift 定向 nextest 本次 Quit 测试 1/1 通过。

`save_project_as_matches_native_ctrl_alt_s_and_opens_the_save_picker` 验证原厂 Ctrl+Alt+S 解析为 SaveProjectAs，并在应用更新后进入既有保存选择器状态、关闭主菜单；本轮 AAADAW 全量 nightly+Cranelift 测试 555/555 通过。

REAPER Main action 41895 在 File 菜单显示 `Save new version of project`、快捷键 Ctrl+Alt+Shift+S；Action List 名称是 `File: Save new version of project (automatically increment project name)`。Linux REAPER 7.82 / Default 7 探针从 `save-version-probe.rpp` 连续执行两次，当前工程路径依次变为 `_001`、`_002` 版本。AAADAW 现注册相同菜单名、Action List 名称和默认快捷键；保存已有工程时从 `_001` 开始递增，跳过已有版本文件，沿用现有原子保存和媒体复制流程，成功后切换活动路径；未命名工程转入保存选择器。原厂探针、两张菜单图、截图哈希和回归范围见 [Save new version fixture](fixtures/menu-save-new-version-aaadaw.tsv)。

四项回归覆盖菜单/Action List/快捷键映射、命令派发、版本号补零与冲突跳过，以及保留原文件并保存当前项目状态。实窗截图验证 File 菜单短标签、快捷键和单行呈现；原厂同尺寸 File 菜单截图显示该标签和快捷键。完整 File 菜单仍有大量命令缺失；版本号边界、其它工程格式、Action List 完整窗口视觉和整体菜单对齐仍未完成，`SHELL-MENU-KEYBOARD-001` 保持 `in_progress`。

Linux REAPER 7.82 的 File 菜单和 Action List 还提供没有默认快捷键的 `Save all projects`（40897）与 `Close all projects`（40886）。原厂探针在一个已保存工程里修改并执行 Save all，确认当前活动路径的工程文件包含新增轨道；随后 Close all 关闭唯一工程，REAPER 留在空白未保存工程工作区。AAADAW 已将两项分别接到现有当前工程 Save 与带 Save/Don't Save/Cancel 保护的 Close 流程，菜单次序与原厂相符。原厂结果、同屏 File 菜单截图、哈希和验证范围见 [Save/Close all fixture](fixtures/menu-save-close-all-aaadaw.tsv)。

回归验证两个 Action List 分类/名称和无快捷键状态、File 菜单顺序、保存当前已命名工程、取消关闭保留修改以及确认放弃后回到空工程。实窗截图确认两个入口与分隔线的位置；原厂截图确认标签和默认无快捷键。AAADAW 暂无多工程标签模型，Save all / Close all 在多开工程场景尚未实现，完整 File 菜单和界面仍为 `in_progress`。

REAPER Main action 40015 的 Action List 名称为 `File: Render project to disk...`、默认 Ctrl+Alt+R，File 菜单显示 `Render...`。AAADAW 现在用该原厂短标签和快捷键打开已有渲染窗口；Action List 分类/名称也与原厂一致。原厂与 AAADAW File 菜单截图、版本和 SHA-256 见 [Render action fixture](fixtures/menu-render-action-aaadaw.tsv)，快捷键回归同步检查菜单与 Action List 注册。当前渲染实现仍只覆盖 WAV；REAPER 其它输出格式、Render to File 完整控件和逐项视觉对齐尚未完成。

File 菜单的 Main action 40929 `File: Open render queue` 没有默认快捷键，原厂短标签为 `Queued Renders`。AAADAW 为其注册独立 Action List 项和同名菜单标签，执行时打开已有的 Offline Jobs 面板；重复执行保持面板打开，View 菜单中的 Toggle Offline Jobs 则继续负责开关。实窗菜单、原厂菜单参考、哈希和测试范围见 [Render queue fixture](fixtures/menu-render-queue-aaadaw.tsv)。原厂 Render Queue 与 AAADAW Offline Jobs 面板的内容/布局还未对齐。

`close_project_matches_native_menu_and_preserves_unsaved_work_until_confirmed` 覆盖 File 菜单标签/快捷键、Action List 名称、修改后取消，以及 Don't Save 后关闭工程并清空当前轨道、路径和修订号。`successful_save_before_close_project_clears_the_open_project` 覆盖 Save 成功后继续关闭。实窗 Ctrl+F4 确认对未保存工程弹出保护框，步骤和哈希见上方 fixture。

Help 仍未复刻原厂 Documentation 子菜单、Keyboard bindings and mouse modifiers、HTML 格式 Action List、ReaScript 文档、EULA、购买/更新检查和 changelog；About 信息窗口也尚未做原厂逐像素对照。Options 其余大量设置、Track/Item 菜单内容、所有禁用/勾选状态和文本焦点行为也未验收。该条目仍为 `in_progress`。

### Item 菜单的空工程 Select all

Linux REAPER 7.82 / Default 7 原厂窗口在一条空轨、没有媒体项时仍将 Item > Select all 显示为启用；AAADAW 的同状态实窗也显示启用的 `Select all`，位于 `Remove items`、`Trim items to selected area` 和分隔线之后。原厂将它放在多个 AAADAW 尚未实现的命令之后，故菜单顺序仍不一致；原厂菜单中其它依赖媒体项的命令也按其自身状态禁用。原厂与 AAADAW 截图、版本、空工程状态和 SHA-256 见 [Item Select all fixture](fixtures/menu-item-select-all.tsv)。两张图尺寸、菜单顺序和余下内容不同，本次只验收命令名称、空工程可用状态与执行结果。

`item_menu_exposes_native_remove_and_select_all_availability_for_an_empty_project` 验证 `Select all` 在零媒体项时可用，执行后仍无选择且不修改工程 revision；选择动作本身对空工程是安全 no-op。此切片没有补齐 REAPER Item 菜单的其它命令或子菜单，整项菜单一致性仍为 `in_progress`。

### Item 菜单的 Remove items

REAPER 7.82 / Default 7 在没有所选媒体项时将 `Remove items` 放在 Item 菜单首项并禁用。AAADAW 现将已有的删除所选项动作放到菜单首项，使用原厂名称，并在空选择时禁用；`Trim items to selected area` 接在其后，启用；`Select all` 位于后续分隔线之后。选择 MIDI 项后 Remove items 启用，执行删除作为一次可撤销工程编辑；Action List 的原有 “Delete selected items” 搜索名称保留为别名。原厂截图复用空工程 Item 菜单探针，AAADAW 截图、哈希与回归范围见 [Remove items fixture](fixtures/menu-item-remove.tsv)。截图只对比空选择状态，尺寸及菜单中其余命令仍有差异。

`item_menu_exposes_native_remove_and_select_all_availability_for_an_empty_project` 验证首项名称、顺序、禁用状态及 Select all 的启用状态；`item_menu_remove_items_deletes_selected_media_and_is_undoable` 验证所选 MIDI 项使菜单动作可用、删除和撤销。原厂全选所在位置之前的其它菜单命令仍未实现，整项菜单继续 `in_progress`。

### Item 菜单的 Copy loop of selected area

Linux REAPER 7.82 / Default 7 的空轨 Item 菜单显示启用的 `Copy loop of selected area of items`，快捷键 Ctrl+Alt+L。Main action 40014 复制所选音频项中完整覆盖时间选区的源区段到媒体项剪贴板；单项 0–10 秒、选区 3–7 秒时，复制不改变工程修订、item 属性或选择。随后在 12 秒粘贴会得到 4 秒 item，启用 Loop source，底层仍指向原 WAV 的 3–7 秒 source section。两个音频项同时选中时，只复制完整覆盖整个选区的项；选区只部分伸入的项不进入该剪贴板。

AAADAW 将动作注册为 Action List `Copy loop of selected area of audio items`（`item.copy-loop-selected-area`，Ctrl+Alt+L），Item 菜单显示原厂短标签。音频剪贴板记录当前工程的源引用、样本偏移、区段长度和相对轨道顺序；Edit > `Paste`（Ctrl+V）在播放光标处将这些片段作为一个可撤销编辑插入，并在源工程已有的目标轨道间保留相对顺序。空工程 Copy 仍启用且安全 no-op，空剪贴板 Paste 禁用。真实原厂 ReaScript 结果、原厂菜单截图、AAADAW Item/Edit 实窗图与哈希见 [Copy loop fixture](fixtures/menu-item-copy-loop.tsv)。

`copy_loop_of_selected_audio_area_pastes_source_sections_on_one_undo_step` 用 48 kHz 源与 44.1 kHz 工程的样本换算验证完整选区、部分相交项排除、MIDI 排除、多轨偏移、光标粘贴、一次 Undo 及剪贴板保留。当前剪贴板仅在同一工程代次有效；目标轨道不足时不自动建轨；AudioItem 尚无 Loop source 状态，因此目前不能验证粘贴项后续延长时的循环、淡化/增益/播放率保留和跨工程媒体引用。原厂对照也尚未覆盖这些属性，本切片和 `ITEM-AUDIO-COPYLOOP-001` 保持 `in_progress`。

### Item 菜单的 Enclose media items in razor edit

Linux REAPER 7.82 / Default 7 的 Main action 42409 会把选中的音频与 MIDI 素材包在轨道级 media razor area 中。原厂探针选择了 A 轨的 2–4 秒、6–7 秒音频项和 10–12 秒 MIDI 项，以及 B 轨的 4–6 秒音频项；动作在 A、B 两轨都写入 2–12 秒区域，包含素材之间的空隙，并取消四个素材项的选择。C 轨不受影响。对单个 14–16 秒的未选中素材再次执行，会替换此前的区域；没有所选素材时则清除全部区域。该次执行只产生一个工程状态变更，Undo/Redo 清除和恢复区域。原厂探针结果及撤销边界见 [Enclose 原厂 TSV](fixtures/menu-item-enclose-razor-native.tsv)。

AAADAW 将完整命令注册为 Action List 的 Razor edit 命令，在 Item 菜单显示 `Enclose media items in razor edit`，位于 Copy loop 之后、Select all 分隔线之前。执行时取所有所选音频与 MIDI 项的全局起止范围，在每条含所选项的轨道写入同一 media area，并清除其它轨道的旧区域；无选择时清空所有轨道区域。区域以样本时钟保存在工程 schema 41，参与 Undo/Redo，并在时间线上按轨道显示半透明覆盖。App 回归覆盖多轨混合媒体、跨项空隙、清除旧区域、Undo/Redo 和零选择清除；核心及存储回归覆盖快照往返、非法区域原子拒绝和迁移。原厂和 AAADAW 的轨道画面见[原厂区域截图](fixtures/menu-item-enclose-razor-native.png)及[AAADAW 区域截图](fixtures/menu-item-enclose-razor-aaadaw.png)；Item 菜单图、环境、精确哈希与比较范围见[Enclose fixture](fixtures/menu-item-enclose-razor.tsv)。两张画面使用相同轨道数、素材时间与 2–12 秒区域，验证区域覆盖和空隙，不表示整体界面像素一致。

本切片只实现基础 media razor area 的创建、持久化与显示；尚无 envelope razor area 或其它 Razor edit 操作。AAADAW Undo 会恢复区域，但目前不会恢复执行前的 item 选择状态；原厂探针由 ReaScript 构造工程，其 Undo 后选择结果受构造历史影响，尚不能用于确认此项差异。`ITEM-RAZOR-ENCLOSE-001` 和完整 Item 菜单/界面仍保持 `in_progress`。

### Item 菜单的 Trim items to selected area

REAPER 原厂 40508 动作将所选素材裁到时间选区与素材的交集；完全落在选区外的素材保留，恰好与选区端点相接的素材删除，没有时间选区时不修改工程。Linux REAPER 7.82 / Default 7 的 ReaScript 探针记录了内部裁切、左右部分交叠、选区外、两端贴边和无选区结果。AAADAW 将命令接到 Item 菜单，空选择时仍启用，与原厂动作可从空工程菜单执行的状态一致；裁切和端点删除在单个可撤销事务中应用，并保留音频/MIDI 源偏移及被裁后仍存活项的选择状态。

原厂操作结果、原厂裁切界面截图、空工程原厂菜单截图、AAADAW Item 菜单实窗图、哈希和回归范围见 [Trim items fixture](fixtures/menu-item-trim.tsv)。截图只验证菜单项可见性与空工程可用状态，窗口尺寸、完整菜单内容及画布几何仍有差异；AAADAW 的音频/MIDI 行为由 `trim_items_to_selected_area_clips_audio_and_midi_in_one_undoable_edit`、`trim_items_to_selected_area_matches_native_overlap_and_endpoint_rules` 与 `trim_items_to_selected_area_without_a_selection_is_a_noop` 验证。Item 菜单现在还按原厂顺序包含 Copy loop 与 Enclose media 命令；其它尚未实现的菜单项和完整界面对照仍为 `in_progress`。

### Item 菜单的 Split items at time selection

Linux REAPER 7.82 / Default 7 的 Main action 40061 名为 `Item: Split items at time selection or razor edit`，默认快捷键 Shift+S。原厂探针确认三种模式：没有时间选区时，动作按各轨道的 media razor area 两端切分该轨道上的所有音频/MIDI 项、清除这些 area，并只选择完全落在 area 内的片段；有时间选区且已有项选择时，只切分所选项并只选择时间选区内部的片段；有时间选区但没有项选择时，则切分整个工程中的项并选择选区内部片段。未受当前模式影响的素材保持原样，每次执行只增加一个工程状态变更。分割结果和原厂环境记录在 [Split 原厂 TSV](fixtures/menu-item-split-time-or-razor-native.tsv)。

AAADAW 的 Action List 使用完整原厂动作名，Item 菜单显示 `Split items at time selection` 和 Shift+S。它实现上述三种路径：音频边界按样本时钟切分并换算源偏移，MIDI 按 tick 边界切分；razor 路径只处理带 media area 的轨道并在同一撤销事务中清除区域。混合音频/MIDI、时间选区无项选择时的全工程分割、选中模式下不触碰未选中轨道、Razor 轨道范围和单步 Undo/Redo 均有应用回归覆盖。REAPER 与 AAADAW 的原厂结果时间线分别见[原厂分割截图](fixtures/menu-item-split-time-or-razor-native.png)和 [AAADAW 分割截图](fixtures/menu-item-split-time-or-razor-aaadaw.png)；菜单截图、哈希、生成工程方式和比较范围见 [Split fixture](fixtures/menu-item-split-time-or-razor.tsv)。两张时间线截图使用相同的四条轨道及 0–10 秒素材，显示首轨 3–7 秒中段被选中，其他轨道素材未改变；截图只验收该动作结果与核心标签/快捷键，不代表整套菜单及界面已像素对齐。

时间选区同时存在 razor area 时，AAADAW 目前优先使用时间选区；该组合尚未由原厂探针确认。撤销工程分割会恢复素材，但应用当前没有通用的 item 选择历史，因此不能承诺 Undo/Redo 后的选择状态与 REAPER 一致。其它 Razor grouping、envelope area 和完整 Item 菜单仍未实现，`ITEM-SPLIT-TIME-RAZOR-001`、`ITEM-RAZOR-ENCLOSE-001` 与菜单/界面整体继续保持 `in_progress`。

### Item 菜单的 Split items at cursor

Linux REAPER 7.82 / Default 7 的 Main action 40012 在 Action List 中显示 `Item: Split items at edit or play cursor (select right)`，默认快捷键 S；Item 菜单短标签为 `Split items at cursor`。原厂 ReaScript 探针确认：已有项选择时只分割所选的跨光标素材，并取消左段选择、只选择右段；没有项选择时分割所有轨道中跨光标的音频/MIDI 项，并选择新生成的右段；光标落在项边界上不产生工程变更。每次有效分割只产生一次 project state change，结果见[原厂 TSV](fixtures/menu-item-split-cursor-native.tsv)。

AAADAW 的 Item 菜单保留 `Split items at cursor` 和 S，Action List 名称现与原厂 action text 对齐。所选项路径仅选择新增右段；空选择路径遍历全部项目并用正确的源采样率切分音频、按 tick 切分 MIDI。回归验证右段选择、跨轨无选择切分、边界无操作、音频源偏移、Undo/Redo 工程状态和快捷键/Action List 名称。REAPER 与 AAADAW 的实窗结果见[原厂图](fixtures/menu-item-split-cursor-native.png)和[AAADAW 图](fixtures/menu-item-split-cursor-aaadaw.png)；环境、图片/TSV 哈希、实际 S 键输入以及截图中的 5.000s / 5.007s 光标差见[Split cursor fixture](fixtures/menu-item-split-cursor.tsv)。两张图均显示两条 MIDI 轨道的右段被选中；AAADAW 时间尺按屏幕像素定位，因此实窗输入落在 5.007s，严格功能边界由 tick 级回归覆盖。

本切片尚未对音频项做 REAPER 原厂实窗截图验证，也没有实现撤销后恢复 item 选择；其它 Razor 行为、Item 菜单未实现项和整体界面复刻仍在 `in_progress`，对应验收条目为 `ITEM-SPLIT-CURSOR-001`。

### Item 菜单的 Heal splits in items

Linux REAPER 7.82 / Default 7 的 Main action 40548 在 Item 菜单显示 `Heal splits in items`，无默认快捷键；没有已选媒体项时菜单禁用，选择媒体项后启用。原厂探针显示该动作只合并同轨、时间相接且源偏移连续的已选音频片段；两段都要选中。只选左段或右段、无选择、MIDI 分段、时间有间隙或源偏移不连续都不改动工程。两段或连续三段全选时一次动作分别合成一个音频项，只增加一次工程状态变更；合并结果保留左侧项的起点和源偏移，并继续选中。完整边界见[原厂结果 TSV](fixtures/menu-item-heal-splits-native.tsv)。

AAADAW 的 Action List/Item 菜单均显示 `Heal splits in items`，菜单启用态与原厂一致。执行只考虑同轨连续的相邻所选音频项，再按工程/源采样率验证源帧连续性；连续项被合并为一项，保留左侧 item ID 与工程模型中的媒体属性，右侧被删除，其它仍存在的选择项保持选中。不同媒体引用、时间 gap、MIDI 或单独一段都不产生工程修改。44.1 kHz 源在 48 kHz 工程中的偏移换算、两段/三段合并、单步 Undo/Redo 和菜单状态均有回归覆盖。原厂是否允许不同媒体引用的相邻音频项合并，仍需单独验证。

REAPER 原厂菜单截图、合并结果、AAADAW 实际选中两段并从 Item 菜单运行后的菜单与时间线截图、环境及 SHA-256 见 [Heal splits fixture](fixtures/menu-item-heal-splits.tsv)。截图只比较菜单文字/无快捷键、选中音频段合成单项的结果，不代表 Item 菜单其它条目或整体 UI 已对齐。

REAPER 原厂探针确认 40548 对 MIDI split 不生效；目前只实现基础 audio split healing。自定义 item fades/mix 的 REAPER 分割与愈合恢复语义、grouped-item 影响和冻结项等边界仍待专项探测；`ITEM-HEAL-SPLITS-001` 与完整 Item 菜单/界面继续保持 `in_progress`。

### Item 菜单的 Nudge/set items...

Linux REAPER 7.82 / Default 7 的 Item 菜单在 `Select all` 后显示 `Nudge/set items...` 和 `N`；Main action 41226 的 Action List 名称为 `Item edit: Nudge/Set...`。原厂菜单截图与 Action List 探针分别见 [Item Select all 原厂图](fixtures/menu-item-select-all-native.png)和 [原厂 actions.tsv](linux-7.82/actions.tsv)。

AAADAW 现在用同一个命令 ID 注册 Action List 与 Item 菜单入口，Action List 保留原厂完整名称，菜单显示短标签和 N。实际运行的 AAADAW 主窗口截图见 [Nudge 菜单 fixture](fixtures/item-nudge-aaadaw-menu.png)。对话框基础位置操作、原厂 Nudge 结果、原厂/AAADAW 实窗截图及差异见 [Nudge/Set 验收记录](item-nudge.md)。

菜单标签、位置和 N 快捷键已做实窗对照；Item 菜单中缺失的命令、其它禁用状态和整体菜单布局仍未对齐。Nudge 的音频边界目标、非循环源边界约束、AAADAW 内的循环源 Contents 绕回与 Duplicate 已覆盖一部分；REAPER 循环边界、MIDI 目标、Set/Snap 组合和 Undo 行为仍需原厂验证，`ITEM-NUDGE-SET-001` 与 Item 菜单/UI 总体保持 `in_progress`。
