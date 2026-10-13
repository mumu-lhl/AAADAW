# Main menu keyboard navigation

## REAPER 7.82 reference

在 Linux REAPER `7.82/linux-x86_64`、Default 7 和 Dummy Audio 下，用隔离 X11 输入向主窗口发送按键。F10 不打开菜单；F10 后的 Enter 仍执行默认 Play 动作。原厂 `Alt+F/E/V/I/O/A/H` 分别打开 File、Edit、View、Insert、Options、Actions 和 Help。重复按 `Alt+F` 时 File 菜单保持打开。探测的 `Alt+T` 和 `Alt+M` 不打开菜单；Track 与 Item 菜单可从已打开菜单用左右方向导航到达。逐项结果见[原厂 TSV](fixtures/menu-navigation-native.tsv)，菜单截图分别为[File](fixtures/menu-file-native.png)、[Edit](fixtures/menu-edit-native.png)、[View](fixtures/menu-view-native.png)、[Insert](fixtures/menu-insert-native.png)、[Options](fixtures/menu-options-native.png)、[Actions](fixtures/menu-actions-native.png)和[Help](fixtures/menu-help-native.png)。

## AAADAW 当前实现与差异

主窗口按原厂已验证的 `Alt+F/E/V/I/O/A/H` 打开 File/Edit/View/Insert/Options/Actions/Help 菜单；快捷键只在 Iced 将按键标为未消费时触发。同一访问键会重新打开对应菜单，不会意外将它关掉。F10 不再打开菜单。主菜单栏和左右导航现有原厂九项顺序。Options 包含 Preferences...（Ctrl+P）和“Loop points linked to time selection”开关；后者显示当前状态勾选，执行后通过现有全局偏好保存路径持久化，不修改工程修订。Help 有“Action list and shortcuts...”入口和单实例 About AAADAW 窗口，`Ctrl+F1` 打开 About。

回归 `options_menu_exposes_preferences_and_updates_the_loop_link_checkmark` 验证菜单命令、快捷键绑定可用、勾选态切换、全局偏好排队保存和工程 revision 不变；`help_menu_opens_the_action_list_and_a_single_about_window` 验证帮助入口、About 复用同一窗口、关闭清理和 Ctrl+F1；菜单键盘回归覆盖 Alt+O/Alt+H 与九项左右顺序。nightly+Cranelift 定向 nextest 6/6 通过。

Help 仍未复刻原厂 Documentation 子菜单、Keyboard bindings and mouse modifiers、HTML 格式 Action List、ReaScript 文档、EULA、购买/更新检查和 changelog；About 信息窗口也尚未做原厂逐像素对照。Options 其余大量设置、Track/Item 菜单内容、所有禁用/勾选状态和文本焦点行为也未验收。该条目仍为 `in_progress`。
