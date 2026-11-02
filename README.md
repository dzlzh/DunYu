<p align="center">
  <img src="resources/fish.png" width="144" height="144" alt="遁鱼图标：荷叶间游动的橙色锦鲤">
</p>

<h1 align="center">遁鱼 DunYu</h1>

<p align="center"><strong>摸鱼有道 · 隐现自如</strong></p>
<p align="center">用快捷键召唤程序、隐藏窗口、调整透明度的 Windows 桌面工具。</p>
<p align="center"><a href="docs/USAGE.md">使用说明</a> · <a href="#快速开始">快速开始</a> · <a href="#开发状态">开发状态</a></p>

## 功能

| 功能 | 当前实现 |
| --- | --- |
| 程序召唤 | 为程序配置全局快捷键；目标在前台时隐藏，否则恢复并激活，找不到窗口时启动程序。支持启动参数与窗口匹配条件。 |
| 窗口绑定 | 四个临时槽位，分别绑定、显示或隐藏窗口；绑定在当前会话内有效。 |
| 透明度控制 | 滑块或快捷键调整 10%～100% 不透明度，支持长按连续调节、自定义步长及恢复。 |
| 快捷键设置 | 按键直接录入，支持 Ctrl、Alt、Shift、Win 组合；保存时检查重复及注册冲突。 |
| 托盘与恢复 | 驻留托盘、单实例、恢复受控窗口状态、可选的当前用户开机自启动。 |
| 暗色界面 | 召唤、绑定、隐现、设置四页；支持 DPI 缩放、键盘导航和系统高对比模式。 |

通知读取、微信转发及配置导入导出仍在规划中。

## 快速开始

面向 **Windows 10 / Windows 11 x64** 的普通桌面窗口，当前为开发验证版本。

1. 在本仓库 GitHub Actions 的成功 Windows 构建中下载 `DunYu-windows-x64` 产物，解压其中的便携 ZIP；也可按下方说明从源码构建。
2. 将程序放在可写目录，启动 `dunyu.exe`。更新版本前先从托盘退出旧实例。
3. 在“召唤”页新建规则，选择程序并录入快捷键，点击保存。可以通过“捕获窗口信息”填写匹配条件。
4. 关闭设置窗口后程序继续驻留托盘；双击托盘图标重新打开设置，托盘菜单可恢复窗口或退出程序。

> 捕获的标题可能包含文件名。只想按程序召唤时，可在高级选项中清空会变化的标题条件；快捷方式需要填写目标程序的实际进程路径。

各页修改需要保存。Win 组合键能被录入，不代表系统允许注册；系统保留或被其他程序占用时会提示失败并保留原配置。

### 默认快捷键

| 操作 | 快捷键 |
| --- | --- |
| 绑定当前窗口至槽位 1～4 | `Ctrl+Alt+Shift+1～4` |
| 显示／隐藏槽位 1～4 | `Ctrl+Alt+1～4` |
| 降低／提高不透明度 | `Ctrl+Alt+Left` / `Ctrl+Alt+Right` |
| 当前窗口恢复 100% 不透明度 | `Ctrl+Alt+0` |
| 恢复所有受控状态 | `Ctrl+Alt+Backspace` |

程序召唤快捷键由每条规则单独设置。完整操作、窗口匹配与排查说明见 [使用文档](docs/USAGE.md)。

## 从源码构建

项目使用 **Rust 1.96 + 原生 Win32**。Windows 构建需要 Visual Studio C++ Build Tools 和 Windows SDK。

```powershell
cargo test --locked --all-targets
cargo build --locked --release
.\target\release\dunyu.exe
```

GitHub Actions 工作流 配置了 Linux / Windows 测试、Windows 发布构建、便携 ZIP 与 SHA256 校验文件，以及 `DunYu-ui-preview` 界面截图产物。

## 配置与恢复

配置和恢复记录保存在程序旁的 `data` 目录：

- `data/config.json`：规则、快捷键、透明度步长与自启动设置。
- `data/recovery.json`：受控窗口的恢复记录。

窗口控制和配置保存在本地。自启动默认关闭，程序不自动提升权限。受控窗口仍隐藏时，请保留恢复记录；退出前可从托盘选择全部恢复。

## 开发状态

已实现功能仍需 Windows 实机验收：

- [x] 程序召唤、窗口隐藏与四槽位绑定
- [x] 透明度调节及恢复
- [x] 快捷键配置、Win 组合键录入与注册冲突提示
- [x] 暗色界面、托盘、自启动与恢复记录
- [ ] Windows 10/11 实机完整验收，包括 Win 键、连续滚动、跨屏 DPI 和高对比模式
- [ ] Windows 通知读取、微信转发与过滤规则
- [ ] 配置导入导出、多语言支持
- [ ] 正式安装包及 Releases 发布

当前已通过 11 项 Linux 核心测试和 Windows x64 GNU Release 交叉编译。Windows 集成测试程序已编译，尚未在本地 Windows 实机执行；编译成功及截图产物不等同于交互验收通过。

## 图标

图标以提供的锦鲤荷塘图片为参考：深青色水面、橙色锦鲤与对角分布的绿色荷叶，外缘为透明圆角。新图标统一用于软件文件、窗口、任务栏、托盘和界面标识。

[PNG 源图](resources/fish.png) · [Windows 多尺寸 ICO](resources/dunyu.ico) · [生成提示词](resources/icon-prompt.txt)

## 项目名称与许可

**遁**，隐匿其形，来去自如；**鱼**，悠游自在，不拘一处。

License 待定。
