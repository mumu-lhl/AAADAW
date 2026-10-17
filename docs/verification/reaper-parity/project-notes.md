# Linux REAPER 7.82 工程 Notes

本次覆盖 Project Settings → Notes 的标题、作者、正文、Word wrap、Show notes on project load。固定参照为 Linux / 7.82 / Default 7 / 新原厂配置。[自有探针](../../../scripts/reaper_parity/probe-project-notes.lua) 与[文本/数值记录](project-notes-reference.json) 保存实测规则，不提交原厂截图、资源或导出的原厂工程。

正文编辑期间不修改工程；Cancel 丢弃草稿，OK 接受标题、作者和正文。原厂 Ctrl+Z 不撤销已经接受的正文，Notes 修改也不增加工程媒体状态计数。默认标题/作者/正文为空、Show notes on project load 为 false、Word wrap 为 true。原厂导出 NOTES 头默认为 0/2；选择显示、关闭折行后为 1/1。加载自有已保存工程时，显示选项打开 Project Settings 的 Notes 页，字段与选项恢复。原厂输入产生 CRLF；AAADAW 未编辑的正文原样保留，编辑后的正文使用编辑器实际行尾。

实现新增公开 ProjectNotes / Project::project_notes / DawAction::SetProjectNotes 与 Snapshot。Notes 在领域动作或嵌套事务中原子应用；写入媒体历史前移除 Notes 事件，因此它不会清空已有 Redo，也不随后续 Undo/Redo 回退。混合帧率/Notes 设置仍原子提交，媒体 Undo 只回退帧率。UI 使用独立草稿与多行编辑器，正文 Enter 属于编辑器；切换 Video/Notes 不丢失草稿，Cancel、过期工程及忙碌守卫沿用工程设置路径。保存工程打开与会话恢复读取 show_on_load。

schema 28 独立 project_notes 单行表保存全文和两个严格 0/1 选项。旧工程迁移默认空说明与原厂显示选项；重复打开幂等、只读旧工程不写入、非法选项失败关闭、保存失败整笔回滚。旧应用拒绝未来版本；回滚使用迁移前备份，不编写降级器。

当前两页尚不代表原厂全部 Project Settings 页已完成；完整标签几何、焦点与快捷键、多项目标签和模板默认设置仍按矩阵推进。实际 GUI 与最终验证见下文。

## 本轮验证

通过 RTK 执行 `cargo xtest --features aaadaw/audio-device --no-fail-fast`：861 passed / 0 skipped。默认 workspace Clippy warnings denied、fmt/diff 通过。领域与存储测试验证已有 Redo 不被 Notes 清空、Undo/Redo 保持当前 Notes、含失败动作的混合事务回滚、混合帧率事务只撤销帧率、Snapshot/Unicode/CRLF 与空行恢复、27→28 默认迁移和媒体保留、重复打开、只读旧文件字节保护、插入失败整笔回滚、非法显示选项拒绝。

真实 AAADAW X11：正文 Enter 后窗口保持打开；[Cancel 后重开](project-settings-ui/notes-cancelled.png)为空默认值；编辑标题/作者/两行正文和选项后切到 Video 选择 25 fps，再返回 Notes，全部草稿保持（[提交前](project-settings-ui/notes-ready.png)）。OK 写入 schema 28，标题 `Owned title`、作者 `Owned author`、正文 `First line\nSecond line`，show_on_load=1、word_wrap=0、帧率编码 2。一次 Ctrl+Z 帧率变 5，Notes 全部保持；Redo 帧率恢复 2。终止本次自有进程并重启后，Recover 自动打开 Notes 页，[字段及选项全部恢复](project-settings-ui/notes-recovered.png)。截图只包含 AAADAW 自有界面。

RTK 已安装 0.51.0 并通过安装包 SHA-256；开发命令使用 RTK 的 Git/rg/Cargo/test 压缩，必要的精确读取/图形命令经 proxy 执行。过滤后保留测试退出状态，遇到缺失细节仍读取原始证据。工具安装位于执行环境，不作为项目依赖或资源提交。
