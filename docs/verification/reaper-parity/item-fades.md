# Audio Item 手动淡化基础

参照：Linux REAPER 7.82 / Default 7 / 独立初始配置，2026-10-10。
矩阵：`ITEM-FADE-001`，仍为 `in_progress`；此记录不代表 Item Properties、自动 Crossfade 或鼠标行为已经完整对齐。

## 实机行为与逐帧证据

自有 [Lua 探针](../../../scripts/reaper_parity/probe-item-fades.lua)只在空白、未保存的独立工程中创建轨道、插入自有常量媒体并离线渲染。版本、主题与空白工程均有守卫。输入是 48 kHz / 单声道 / 16 位 / 一秒、幅度 0.125 的 WAV；输出为 48 kHz / 双声道 / 24 位 / 48,000 帧，关闭 Tail。未提交原厂资源、截图或音频。

API `AddMediaItemToTrack` 创建路径默认淡入/淡出各 0.01 秒；界面媒体插入路径 `InsertMedia` 则默认两者均为零。两条路径的默认形状均为 1。自动长度与方向均为零。不能把 API 创建默认长度错误应用到每次文件导入。

归一化位置 `x` 从零到一，七种手动淡入曲线实测为：

| 原厂编号 | 曲线 |
| --- | --- |
| 0 | `x` |
| 1（默认） | `x * (2 - x)` |
| 2 | `x²` |
| 3 | `1 - (1 - x)⁴` |
| 4 | `x⁴` |
| 5 | `3x² - 2x³` |
| 6 | 前半段 `8x⁴`；后半段 `1 - 8(1 - x)⁴` |

淡出使用剩余时长归一化；一秒 Item 的最后一帧仍在 Item 内，线性 0.25 秒淡出该帧的增益为 `1 / 12000`，Item 的排他结束点为零。

请求长度保留原值，渲染有效长度满足：`in = min(requested_in, Item_length)`；`out = min(requested_out, Item_length - in)`。淡入优先。0.75/0.75 秒请求在一秒 Item 上实际成为 0.75/0.25 秒；1/1 或 2/0 秒请求均为整段一秒淡入。两条重叠手动曲线不会相乘。通过 `UpdateItemInProject` 刷新后重复渲染，规则不变。

[逐帧校验脚本](../../../scripts/reaper_parity/verify-item-fades.py)检查七种曲线及五组超长/重叠案例的全部 1,152,000 个声道采样点；归一化增益最大误差小于 `4.768e-7`，小于一个 24 位 PCM 量化步长。每个输出的散列和误差见 [事实记录](item-fade-reference.json)。常量幅度归一化，因此此处不是主观听感或少数截图比较。

复验时为每次探针创建独立配置与输出目录，生成上述自有输入 `source.wav`，设置 `AAADAW_REAPER_PROBE_DIR`，用 REAPER 的 `-cfgfile`、`-newinst`、`-nosplash` 启动该 Lua；随后运行 `python3 scripts/reaper_parity/verify-item-fades.py <输出目录>`。原厂可执行文件及输出只存于工作区 scratch。

## 实现边界

`AudioFade` 验证有限非负的小数采样长度，`FadeShape` 保持原厂 0–6 编号；`AudioItemFades` 计算 Item 相对时钟上的增益。新导入和旧工程默认零长度、形状 1。请求超长长度保留，修改 Item 长度时重新计算有效淡化。

公开 `Project` / `SetAudioItemFades` 支持两端参数一次撤销；移动、修剪、快照和完整音频 Item 复制保留参数。普通复制和 Ctrl 拖动复制使用核心完整复制动作，不预测新 Item ID。

schema 25 新增 `audio_item_fades` 表；旧 Item 没有行时恢复零长度。写入在工程事务内，删除 Item 时同步清理。加载验证形状、有限非负长度与孤立行；只读加载不修改原文件。

真实 PCM 在 source 相加到轨道 FX 输入前应用一次淡化，时钟不受媒体 source offset 影响；处理涵盖 Item 起止、跨 callback 与中途 seek，不改变未放置 Item 的传统连续轨道源及输入监听路径。

尚待：GUI 淡化手柄及曲线菜单、播放中的参数发布、原厂手势/修饰键、自动 Crossfade、自动分割淡化、Take/Item gain 和 Item Properties 全界面。基础模型不宣称这些能力已经完成。

## 本地验证

默认首轮 `cargo xtest --no-fail-fast`：794 项全部通过；随后包括新增集成测试的 `cargo xtest --features aaadaw/audio-device --no-fail-fast`：819 项全部通过、零跳过。默认 `cargo clippy --workspace --all-targets -- -D warnings` 通过。音频构建仍有既存 unused_mut/dead_code 两项警告，未宣称该 feature 下 warnings denied 通过。

回归覆盖真实双声道 PCM、七种曲线、非零 Item 起点及 source offset、跨 13 帧 callback、淡出区域 seek、小数/超长长度、Undo/Redo、完整复制、Snapshot、批处理失败原子性、schema 24 默认迁移、只读重开字节不变与非法长度拒绝。默认已有无分配 callback 回归也通过；本切片没有对 GUI 淡化手柄作未执行的验收。


补充原厂 UI 观察：选中单个音频 Item、F2 打开 Media Item Properties，客户端 526×693。该窗口同时包含 Fade in/out 时间、两组 Curve 字段和曲线按钮；超长淡出仍显示 0:02.000，而 Item Length 显示 0:01.000，与保存请求长度的 API 行为一致。完整属性字段和批量编辑将另行实现和验收，未用简化 Inspector 代替。
