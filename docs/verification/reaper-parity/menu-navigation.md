# Main menu keyboard navigation

## REAPER 7.82 reference

在 Linux REAPER `7.82/linux-x86_64`、Default 7 和 Dummy Audio 下，用隔离 X11 输入向主窗口发送按键。F10 不打开菜单；F10 后的 Enter 仍执行默认 Play 动作。原厂 `Alt+F/E/V/I/O/A/H` 分别打开 File、Edit、View、Insert、Options、Actions 和 Help。重复按 `Alt+F` 时 File 菜单保持打开。探测的 `Alt+T` 和 `Alt+M` 不打开菜单；Track 与 Item 菜单可从已打开菜单用左右方向导航到达。逐项结果见[原厂 TSV](fixtures/menu-navigation-native.tsv)，菜单截图分别为[File](fixtures/menu-file-native.png)、[Edit](fixtures/menu-edit-native.png)、[View](fixtures/menu-view-native.png)、[Insert](fixtures/menu-insert-native.png)、[Options](fixtures/menu-options-native.png)、[Actions](fixtures/menu-actions-native.png)和[Help](fixtures/menu-help-native.png)。

## AAADAW 当前实现与差异

主窗口现在按原厂已验证的 `Alt+F/E/V/I/A` 打开现有 File/Edit/View/Insert/Actions 菜单；快捷键只在 Iced 将按键标为未消费时触发。同一访问键会重新打开对应菜单，不会意外将它关掉。F10 不再打开菜单。已有左右方向键、Enter、Escape 和 Actions 搜索的路由保持使用当前菜单焦点状态。

现有 `MainMenu` 仍缺原厂 Options 与 Help 菜单；因此 Alt+O/Alt+H、主菜单栏九项完整布局、Options/Help 命令、Track/Item 的菜单内容及全菜单键盘可达性仍未完成。该条目仍为 `in_progress`，这次只纠正已验证的访问键和 F10 差异。
