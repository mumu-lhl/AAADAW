# 隔离插件预听 CI 时序回归

2026-10-09，提交 0b7cada 的 Windows job 114057801801 / Rust CI 38000707705：WASAPI policies 步骤中的 lib 测试 isolated_instrument_route_accepts_stopped_midi_preview_requests 失败，其余 93 项成功。Linux、macOS、Android 构建和 emulator、Portable 与 Installers 成功。

失败日志：events 1、heartbeat 27、fault code 0、pending false、queued 13、audio/output 0。该测试只执行 100 次 render_into + yield_now；整组 94 项运行约 0.02 秒，不能保证辅助线程获得等同真实设备回调间隔的调度机会。IPC 的过期策略会丢弃未及时完成的响应，因此单纯固定轮数不构成可信的异步完成期限。

测试改为最多 2 秒的墙钟期限，在无音频时由测试 harness 等待 1 ms，再调用仍然非阻塞的 render_into；保留实际音频、预听事件、停止传输位置和辅助进程退出断言。生产回调没有加入 sleep、等待或重试。Linux focused nextest 和 20 次 stress iterations 均通过；Windows 远端复验仍待本次 push 后完成，不提前声称故障已在 Windows 关闭。
