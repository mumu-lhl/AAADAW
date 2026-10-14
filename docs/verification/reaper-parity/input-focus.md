# Input focus and shortcut routing

## REAPER 7.82 reference

在 Linux `7.82/linux-x86_64`、`Default_7.0.ReaperThemeZip` 与 Dummy Audio 下，对未保存的临时工程进行手动键盘探针。用原厂动作 40001 插入第二轨，再运行 40696 `Track: Rename last touched track`，让轨道名称编辑器获得焦点。传输保持停止。

焦点内按 Space 后，名称字段仍聚焦且播放未启动；按 `A` 后字段显示 `a`。焦点内按 `Ctrl+Z` 会执行全局 Undo，撤销最近的插轨操作，轨道数从 2 回到 1。另一次名称编辑中按 Escape 会关闭编辑器，标尺上原有的括线仍可见。逐步结果见[原厂 TSV](fixtures/input-focus-native.tsv)，输入前后的原厂界面见[文本输入](fixtures/input-focus-text-native.png)、[全局撤销](fixtures/input-focus-undo-native.png)和[Escape](fixtures/input-focus-escape-native.png)。动作名称/编号来自[固定版 Action 导出](linux-7.82/actions.tsv)。

[ReaScript 探针](../../../scripts/reaper_parity/probe-input-focus.lua)会在停止的临时工程里插入测试轨、打开原厂名称编辑器，并将轨道数、播放状态、名称、范围和工程状态计数写到 `/tmp/aaadaw-input-focus-native.tsv`；键盘步骤需由测试者在编辑器仍聚焦时输入。该脚本用于复现设置和记录状态，TSV 中的观察值来自上述独立手动会话。

## AAADAW 当前实现与验证

主窗口键盘路由尊重 Iced 的事件状态：控件已消费的普通文本不会变成全局快捷键；未消费的 `Ctrl+Z` 会继续走默认 Undo 动作。Escape 进入应用级取消级联，轨道草稿先取消，再考虑清除 Time Selection/Loop points，因此编辑器退出不会抹掉时间范围。

专项回归 `focused_track_name_editor_routes_native_text_escape_and_undo_keys` 覆盖同一分流：捕获的 Space 不生成全局快捷键，捕获的 Escape 取消轨道名称草稿且保留两种范围，忽略的 `Ctrl+Z` 撤销最近的插轨且保留时间范围。该测试通过 nightly+Cranelift nextest 1/1。

## Action List Filter

在同一 Linux REAPER 7.82 / Default 7 隔离会话打开 Action List（40605），点击 Filter 后输入 `undo`，列表显示 `Edit: Undo` 与 `Edit: Show undo history window`。Filter 聚焦时按 `Ctrl+Z`，查询和临时工程均不变：轨道仍为 1 条，传输保持停止，项目状态计数仍为 2。关闭 Action List、把焦点交还主窗口后再次按 `Ctrl+Z`，轨道数降为 0，状态计数升至 3。Filter 末尾输入空格不会启动播放；Filter 聚焦时 Escape 关闭 Action List。逐步状态见 [原厂 TSV](fixtures/action-list-focus-native.tsv)，按键前后截图见 [搜索](fixtures/action-list-search-before-ctrl-z-native.png)、[Ctrl+Z](fixtures/action-list-search-after-ctrl-z-native.png)、[空格](fixtures/action-list-filter-space-native.png)和 [Escape](fixtures/action-list-filter-escape-native.png)。可复现探针为 [probe-action-list-focus.lua](../../../scripts/reaper_parity/probe-action-list-focus.lua)；测试使用隔离未保存工程，Xvfb 下音频设备关闭，传输全程停止。

AAADAW 的 Action List 独立窗口将键盘事件交给 `action_list_keyboard_event`，不会把 Filter 中未消费的 `Ctrl+Z` 送到主工程快捷键。回归 `action_list_filter_focus_contains_global_shortcuts_and_escape_closes_the_window` 验证 Action List 窗口里的 Space 与 `Ctrl+Z` 不运行主窗口动作，而同一 `Ctrl+Z` 到达主窗口后会执行 Undo；Escape 关闭 Action List。Filter 的输入和多词搜索由现有 Action List 查询路径处理。

其它文本字段、AltGr 与非 US 键盘、脚本编辑区、插件窗口、浮动工具、每个 Action Section 对文本焦点的穿透例外尚未逐项验证；INPUT-FOCUS-001 继续 `in_progress`。
