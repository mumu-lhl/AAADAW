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

### Item 菜单的 Trim items to selected area

REAPER 原厂 40508 动作将所选素材裁到时间选区与素材的交集；完全落在选区外的素材保留，恰好与选区端点相接的素材删除，没有时间选区时不修改工程。Linux REAPER 7.82 / Default 7 的 ReaScript 探针记录了内部裁切、左右部分交叠、选区外、两端贴边和无选区结果。AAADAW 将命令接到 Item 菜单，空选择时仍启用，与原厂动作可从空工程菜单执行的状态一致；裁切和端点删除在单个可撤销事务中应用，并保留音频/MIDI 源偏移及被裁后仍存活项的选择状态。

原厂操作结果、原厂裁切界面截图、空工程原厂菜单截图、AAADAW Item 菜单实窗图、哈希和回归范围见 [Trim items fixture](fixtures/menu-item-trim.tsv)。截图只验证菜单项可见性与空工程可用状态，窗口尺寸、完整菜单内容及画布几何仍有差异；AAADAW 的音频/MIDI 行为由 `trim_items_to_selected_area_clips_audio_and_midi_in_one_undoable_edit`、`trim_items_to_selected_area_matches_native_overlap_and_endpoint_rules` 与 `trim_items_to_selected_area_without_a_selection_is_a_noop` 验证。原厂在 Trim 与 Select all 之间还有 Copy loop of selected area 和 Enclose media items in razor edit 等命令，完整 Item 菜单仍为 `in_progress`。
