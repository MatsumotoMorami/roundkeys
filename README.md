# RoundKeys

Windows 下的 Rust 音游输入显示器：左侧圆形转盘 + 7 个圆角键，按住生成向上流动的轨迹，松开后轨迹继续向上消失。适合 beatoraja / IIDX 的 OBS 直播与录制。

## 启动

运行 `dist/roundkeys.exe`。GUI 负责绑定和外观设置，保持程序运行时 OBS 才能接收输入。默认不会自动启动或控制 OBS、beatoraja、IIDX。

配置保存在 exe 同目录的 `config.json`，更改后自动保存；已有配置保留 `.bak`。可单独保存、载入 `profile-mode2.json` / `profile-mode4.json`。

## 手台与键盘

- Windows Raw Input 的后台、非独占监听，同时读取键盘和 Generic Desktop HID joystick/gamepad/multi-axis controller。
- 适配手台的 HID 按钮与转盘轴；不写设备固件，不修改设备模式，不调用占用设备的 DirectInput 独占接口。
- PHOENIXWAN+ LMT 的 Mode 2 与 Mode 4 可能报告不同的设备 ID / 轴 / 按钮；首次使用时请分别绑定并保存配置。固件允许重映射，因此不硬编码编号。
- 点击预览中的键，或绑定页中的 1–7 / 左皿，再点击一个绑定按钮，按一次实体按钮即可绑定。
- 左皿先绑定一个方向，再绑定另一个方向。两种方向共用一个 SCR 轨道，重叠不会重复计数。
- 每个显示元素可绑定最多 4 个输入，所以可保留键盘备用绑定。
- 默认键盘：左皿 L Shift / L Ctrl；7 键 S D F Space J K L。
- 旋转轴处理包含环形绕回、反转、停止超时；在绑定页可调整阈值和松开时间。
- 模式切换、拔出设备会清理断开设备的按下状态，新的 HID 控制器会自动检测。
- 手台具体固件和游戏同时运行的兼容性应在实机绑定后确认；自动化测试不等于实体手台验证。

## OBS 浏览器源（推荐）

1. 保持 RoundKeys 运行。
2. 在 OBS 中添加 **浏览器** 来源，URL 为 `http://127.0.0.1:17842/overlay`。
3. 默认宽 **652**、高 **524**，帧率 **60**；以 GUI 的 OBS 页显示的实际尺寸为准。
4. 默认透明背景，无须抠色或安装 OBS 插件。
5. 调整来源缩放与位置即可。修改布局后可以同步来源宽高；也可保持来源尺寸，由渲染器等比缩放。
6. 用 GUI 的“显示延迟”使按键图层与游戏捕获同步，不影响游戏实际输入。

浏览器源通过 WebSocket 接收带时间戳的完整轨迹，OBS 重连时自动恢复最近历史。后台断开时显示连接提示；轨迹不会一直卡在按住状态。可使用多个浏览器源。服务只绑定 `127.0.0.1`，不对局域网开放。

GUI 预览也可使用 OBS 窗口捕获后裁剪，但浏览器源能独立输出透明图层，且无需把设置窗口放到直播画面。

**演示动画会同步输出到 OBS**，正式录制前关闭。此版本显示实时输入历史，不是读取 beatoraja 的游戏 replay 文件，也不记录到磁盘。

## 外观

圆角、键宽高、键间距、转盘直径和间距、黑白键错层、每键颜色与标签、发光、背景 RGBA、轨迹速度/高度/透明度和轨道底色。提供雾紫、冰蓝、樱桃配色。

## 编译

Rust stable，Windows x64。支持 MSVC 工具链（需要 Visual Studio Build Tools）或 GNU 工具链（需要 MinGW gcc 在 PATH）。

```powershell
cargo test --locked
cargo build --release --locked
# 或运行 build.ps1，编译并复制到 dist
```

`Cargo.lock` 固定依赖。Rust GUI 使用 eframe/egui；HTTP/WebSocket 服务使用 axum/tokio；OBS 的 Canvas 渲染页内嵌在 exe 中，不依赖 CDN、Node 或浏览器扩展。

诊断地址 `/health` 显示输入监听、控制器、OBS 客户端状态；`/state` 为当前图层快照。
端口可通过环境变量 `ROUNDKEYS_PORT` 修改（GUI 会显示实际地址）。`--headless` 只运行输入和服务；`--demo` 开启测试动画。

## 验证

`cargo test` 覆盖键盘重复事件、双方向左皿合并、长按历史保留、捕获绑定和轴绕回。`tests/browser-smoke.cjs` 使用无界面 Chromium 验证 WebSocket、透明图层、8 个显示元素、轨迹移动与错误日志；需要 Playwright。实机手台 Mode 2 / 4 仍需用户完成输入验证。

## 资料

- [GAMO2 PHOENIXWAN+ LMT 手册入口](https://www.gamo2.com/en/product/phoenixwan+lmt/specs.html)
- [GAMO2 IIDXOLLER 模式配置说明](https://support.gamo2.com/en/gamo2/phoenixwan/lmt-classic-iidxoller-options/)

MIT license。
