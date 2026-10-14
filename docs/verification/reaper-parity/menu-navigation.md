# Main menu keyboard navigation

## REAPER 7.82 reference

在 Linux REAPER `7.82/linux-x86_64`、Default 7 和 Dummy Audio 下，用隔离 X11 输入向主窗口发送按键。F10 不打开菜单；F10 后的 Enter 仍执行默认 Play 动作。原厂 `Alt+F/E/V/I/O/A/H` 分别打开 File、Edit、View、Insert、Options、Actions 和 Help。重复按 `Alt+F` 时 File 菜单保持打开。打开 File 菜单后按 ArrowUp 会从顶部转到末项 Quit，但不会运行该命令；选择、传输状态和截图哈希见[菜单边界键 fixture](fixtures/menu-keyboard-boundary-native.tsv)。探测的 `Alt+T` 和 `Alt+M` 不打开菜单；Track 与 Item 菜单可从已打开菜单用左右方向导航到达。逐项结果见[原厂 TSV](fixtures/menu-navigation-native.tsv)，菜单截图分别为[File](fixtures/menu-file-native.png)、[Edit](fixtures/menu-edit-native.png)、[View](fixtures/menu-view-native.png)、[Insert](fixtures/menu-insert-native.png)、[Options](fixtures/menu-options-native.png)、[Actions](fixtures/menu-actions-native.png)和[Help](fixtures/menu-help-native.png)。

## AAADAW 当前实现与差异

主窗口按原厂已验证的 `Alt+F/E/V/I/O/A/H` 打开 File/Edit/View/Insert/Options/Actions/Help 菜单；快捷键只在 Iced 将按键标为未消费时触发。同一访问键会重新打开对应菜单，不会意外将它关掉。F10 不再打开菜单。主菜单栏和左右导航现有原厂九项顺序。打开 File 后 ArrowUp 从首项循环到 AAADAW 当前末个启用项 Project settings...，再按 ArrowDown 回到 New project；实窗截图与状态见[AAADAW 菜单边界键 fixture](fixtures/menu-keyboard-boundary-aaadaw.tsv)。该探针验证循环导航，不表示 File 命令清单已与原厂对齐。Options 包含 Preferences...（Ctrl+P）和“Loop points linked to time selection”开关；后者显示当前状态勾选，执行后通过现有全局偏好保存路径持久化，不修改工程修订。Help 有“Action list and shortcuts...”入口和单实例 About AAADAW 窗口，`Ctrl+F1` 打开 About。

回归 `options_menu_exposes_preferences_and_updates_the_loop_link_checkmark` 验证菜单命令、快捷键绑定可用、勾选态切换、全局偏好排队保存和工程 revision 不变；`help_menu_opens_the_action_list_and_a_single_about_window` 验证帮助入口、About 复用同一窗口、关闭清理和 Ctrl+F1；菜单键盘回归覆盖 Alt+O/Alt+H、九项左右顺序和启用命令首尾循环。nightly+Cranelift 定向 nextest 6/6 通过。

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
