# Main menu keyboard navigation

## REAPER 7.82 reference

在 Linux REAPER `7.82/linux-x86_64`、Default 7 和 Dummy Audio 下，用隔离 X11 输入向主窗口发送按键。F10 不打开菜单；F10 后的 Enter 仍执行默认 Play 动作。原厂 `Alt+F/E/V/I/O/A/H` 分别打开 File、Edit、View、Insert、Options、Actions 和 Help。重复按 `Alt+F` 时 File 菜单保持打开。探测的 `Alt+T` 和 `Alt+M` 不打开菜单；Track 与 Item 菜单可从已打开菜单用左右方向导航到达。逐项结果见[原厂 TSV](fixtures/menu-navigation-native.tsv)，菜单截图分别为[File](fixtures/menu-file-native.png)、[Edit](fixtures/menu-edit-native.png)、[View](fixtures/menu-view-native.png)、[Insert](fixtures/menu-insert-native.png)、[Options](fixtures/menu-options-native.png)、[Actions](fixtures/menu-actions-native.png)和[Help](fixtures/menu-help-native.png)。

## AAADAW 当前实现与差异

主窗口按原厂已验证的 `Alt+F/E/V/I/O/A` 打开 File/Edit/View/Insert/Options/Actions 菜单；快捷键只在 Iced 将按键标为未消费时触发。同一访问键会重新打开对应菜单，不会意外将它关掉。F10 不再打开菜单。Options 现包含原厂 Preferences...（Ctrl+P）入口和“Loop points linked to time selection”开关；后者显示当前状态勾选，执行后通过现有全局偏好保存路径持久化，不修改工程修订。菜单顺序已将 Options 放在 Track 和 Actions 之间。

回归 `options_menu_exposes_preferences_and_updates_the_loop_link_checkmark` 验证菜单命令、快捷键绑定可用、勾选态切换、全局偏好排队保存和工程 revision 不变；`menu_keyboard_uses_native_alt_mnemonics_and_accepts_enter_while_actions_search_has_focus` 覆盖 Alt+O，`keyboard_navigation_switches_menus_and_wraps_enabled_commands` 覆盖现有八项顺序。定向 nightly+Cranelift nextest 5/5 通过。

Help 菜单仍缺失，故完整九项布局及 `Alt+H`、Actions 后的原厂导航仍未完成；Options 中除上述两个可用项目外的大量原厂设置、Track/Item 菜单内容、所有禁用/勾选状态和文本焦点行为也未验收。该条目仍为 `in_progress`，本次只接入已经有实际后端行为的 Options 项目。
