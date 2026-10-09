# AAADAW-INPUT-001 — 快捷键基础能力

日期：2026-10-09。矩阵：`INPUT-BIND-001`、`UI-030`。分支：`feat/reaper-parity`。本批只实现基础能力，不代表完整 Action List 或 REAPER 默认交互已验收。

## 实现

- 新的逻辑键组合模型表示字符、数字、标点、F1–F24、导航键及 Ctrl/Alt/Shift/Super 精确组合；运行事件保留 Iced `Key` 类型，避免功能键与导航键在字符串转换中丢失。
- 每个动作支持多个绑定，以 `; ` 存储，设置页提供追加绑定。相同绑定自动去重，跨动作语义冲突（包括旧 Delete/Backspace 合并绑定）在提交前检查，失败保留原值。
- 保留旧 `Mod` 平台映射及 `Delete/Backspace` 配置。`Ctrl` 与 `Super` 分别表示实际 Control 与 Logo；`Mod` 在 Linux 为 Control，在 macOS 为 Command。`Plus` 和 `Semicolon` 避免配置分隔符歧义。工程 schema 仍为 17，无工程迁移。
- 未修饰 Escape 保留给取消操作，手写配置若试图绑定会明确报错；捕获模式未修饰 Backspace 清空，带修饰 Backspace 可捕获。

## 自动验证

Debian 13 x86_64、Rust 1.99.0；系统依赖在临时用户目录解包并由 pkg-config 引用。仓库未新增工具链、系统依赖或 CI 配置。

| 检查 | 结果 |
| --- | --- |
| `DISPLAY=:100 cargo xtest --no-fail-fast` | 715 passed，0 skipped；独立无窗口管理器 Xvfb，包含真实 CLAP 测试窗口 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 通过 |
| `cargo build -p aaadaw` | 通过 |
| `cargo check --workspace --all-targets --features 'aaadaw/jack-backend aaadaw/pipewire-backend'` | 通过；仍有两个既有 feature-only warning，与本批绑定实现无关 |
| `cargo fmt --all -- --check`、`git diff --check` | 通过 |

新增回归覆盖：组合解析/捕获/序列化/精确匹配、多绑定和标点、实际 Ctrl/Super 区分、功能键事件到 Project 动作、输入消费与窗口归属保护、追加后保留默认绑定、冲突不破坏配置、旧 Delete/Backspace 重叠、保留 Escape、配置保存读取。测试产生的示例工程更新时间已还原，不提交测试副作用。

## GUI 验证

X11 Xvfb :99 / Openbox，AAADAW 主窗口客户端 1280×800、Settings 760×620；使用 `/tmp/aaadaw-ui-smoke/` 独立 XDG 配置与数据目录。默认构建无音频设备，因此本次不验收播放、录音或声音。

1. File → Settings → Keyboard Shortcuts，在 Undo 点击 Add，捕获 Ctrl+Alt+7，显示 `Ctrl+Z; Ctrl+Alt+7`，保留原绑定。
2. 对 Save 捕获同一组合，显示 Save/Undo 冲突，Undo 保持原追加值；Esc 取消捕获后 Save 保持 Ctrl+S。
3. Save changes，配置文件为 `edit.undo\tMod+Z; Mod+Alt+7`。关闭重开使用同一独立目录。
4. 新增轨道，主窗口未消费的 Ctrl+Alt+7 执行 Undo，轨道消失；重新启动后同一组合再次成功。

截图：[追加绑定](keyboard-append.png)、[冲突反馈](keyboard-conflict.png)、[重新启动后的撤销](keyboard-restart-undo.png)。截图只作为当前功能证据，不是与 REAPER 的像素对比。

Openbox/X11 可优先截获某些组合；本次 Ctrl+Alt+F12 未到达捕获框，改用 Ctrl+Alt+7 完成 GUI 流程。应用层 F12 组合由事件回归验证；系统级冲突提示/规避还未实现。

## 剩余差距

参照目录见 [LNX-782-ACTIONS-001](linux-7.82/README.md)。数字键盘位置、物理键/键盘布局语义、左右 Modifier、全部 Action Sections、单独移除某个绑定、原厂冲突确认行为、完整 Action List 和默认动作映射仍待实现/实测。现有跨动作冲突拒绝策略尚未宣称与 REAPER 一致。矩阵保持 `in_progress`，不计入通过率。
