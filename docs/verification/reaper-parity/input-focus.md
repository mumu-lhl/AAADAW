# Input focus and shortcut routing

## REAPER 7.82 reference

在 Linux `7.82/linux-x86_64`、`Default_7.0.ReaperThemeZip` 与 Dummy Audio 下，对未保存的临时工程进行手动键盘探针。用原厂动作 40001 插入第二轨，再运行 40696 `Track: Rename last touched track`，让轨道名称编辑器获得焦点。传输保持停止。

焦点内按 Space 后，名称字段仍聚焦且播放未启动；按 `A` 后字段显示 `a`。焦点内按 `Ctrl+Z` 会执行全局 Undo，撤销最近的插轨操作，轨道数从 2 回到 1。另一次名称编辑中按 Escape 会关闭编辑器，标尺上原有的括线仍可见。逐步结果见[原厂 TSV](fixtures/input-focus-native.tsv)，输入前后的原厂界面见[文本输入](fixtures/input-focus-text-native.png)、[全局撤销](fixtures/input-focus-undo-native.png)和[Escape](fixtures/input-focus-escape-native.png)。动作名称/编号来自[固定版 Action 导出](linux-7.82/actions.tsv)。

[ReaScript 探针](../../../scripts/reaper_parity/probe-input-focus.lua)会在停止的临时工程里插入测试轨、打开原厂名称编辑器，并将轨道数、播放状态、名称、范围和工程状态计数写到 `/tmp/aaadaw-input-focus-native.tsv`；键盘步骤需由测试者在编辑器仍聚焦时输入。该脚本用于复现设置和记录状态，TSV 中的观察值来自上述独立手动会话。

## AAADAW 当前实现与验证

主窗口键盘路由尊重 Iced 的事件状态：控件已消费的普通文本不会变成全局快捷键；未消费的 `Ctrl+Z` 会继续走默认 Undo 动作。Escape 进入应用级取消级联，轨道草稿先取消，再考虑清除 Time Selection/Loop points，因此编辑器退出不会抹掉时间范围。

专项回归 `focused_track_name_editor_routes_native_text_escape_and_undo_keys` 覆盖同一分流：捕获的 Space 不生成全局快捷键，捕获的 Escape 取消轨道名称草稿且保留两种范围，忽略的 `Ctrl+Z` 撤销最近的插轨且保留时间范围。该测试通过 nightly+Cranelift nextest 1/1。

这只是焦点语义的一个原厂入口。Action List/脚本编辑区、其它文本字段、AltGr 与非 US 键盘、插件窗口、浮动工具、每个 Action Section 对文本焦点的穿透例外尚未逐项验证；INPUT-FOCUS-001 继续 `in_progress`。
