# 后台常驻性能基线

解压完整 `DunYu-resident-diagnostics.zip` 到可写目录，退出正常运行的遁鱼，双击 `run-resident-diagnostics.cmd`。无需 Rust 或 Python。默认七个场景各三个独立进程，约 15～20 分钟；保持桌面解锁、电源模式和显示器配置不变。全屏场景会覆盖当前全部显示器，请勿遮挡或操作测试窗口。

快速单轮：`powershell -ExecutionPolicy Bypass -File run-resident-diagnostics.ps1 -Rounds 1`。

场景为未打开界面的宿主静置、界面打开后隐藏、休憩调度、空闲传讯、全屏休憩动画，以及一条/五条程序启动等待。使用临时目录、模拟通知数据库、不会发送消息的模拟发送器；不读取用户通知、不注册全局快捷键、不实际启动外部程序。宿主采用正式消息处理，测试不包含托盘图标、全局键盘钩子和实际网络请求的开销，不能据此声称覆盖完整生产常驻行为。

常规场景预热5秒、测量60秒，启动等待预热1秒、测量5秒，在10秒超时前清理。消息循环阻塞等待消息或阶段截止，不使用旧UI诊断的1ms轮询。外部PowerShell每秒采样工作集、私有内存、GDI/USER对象和进程句柄；应用记录CPU时间与业务计数。CPU百分比以一个逻辑核心为100%，不是任务管理器的整机归一化百分比。消息等待计数仅表示监听线程的100ms等待超时，不含批次接收线程，也不是操作系统总唤醒次数。绘制计数包含绘制请求，不证明实际帧已上屏。

每场景保留原始JSON、逐秒资源样本、stdout/stderr；顶层包含manifest、summary.json、summary.txt和run.log。采样包括预热和清理后2秒，摘要仅使用正式观测区间。清理后保留缓存不自动判为泄漏。单屏机器的multi_monitor_covered=false明确表示未覆盖多屏。构建版本、机器、Windows版本、CPU、显示器矩形、DPI、动画设置和场景参数用于解释结果；实际其它桌面程序会影响窗口枚举成本，应保持一致。

完整结果要求所有进程正常退出、采样成功、报告完整。失败或超时保留已有产物，manifest.complete=false，不能作完整基线。资源API失败不填零。长时间卡顿或环境改变可能使测试失败；请提供整个结果目录。报告含机器名，请分享前自行评估。

同机同环境比较：

```powershell
powershell -ExecutionPolicy Bypass -File run-resident-diagnostics.ps1 -BaselineDirectory OLD -CandidateDirectory NEW
```

生成NEW/comparison.json，输出各指标中位数差值及原统计；参数、环境或轮数不同标记不可比较。不存在自动优化通过门槛，也不沿用旧UI性能协议。首次采样只建立基线，不能宣称优化收益。正式对比应使用三轮结果。

构建：运行现有`scripts/package-ui-diagnostics.ps1`同时产出独立常驻诊断包；CI执行报告自检并上传。交互采样不在无人值守CI自动执行。Windows实机三轮结果仍是性能验收依据。

## 第二轮：绘制剖析与启动查询对比

本轮保留动画效果和刷新周期。多规则启动等待在一轮内共享窗口快照；匹配到窗口后重新验证身份和条件，快照不跨轮保留。单条规则沿用原查询路径。新增`startup_ticks`计数用于检查查询量下降是否来自去重；旧报告缺少该指标时不填零。

在解压目录打开 PowerShell，分别执行：

```powershell
# 只测启动查询；如果包含已保存的基线EXE，逐轮交替运行基线和新版。
.\run-resident-diagnostics.ps1 -Scenarios pending1,pending5 -BaselineExecutable .\dunyu-resident-baseline.exe
# 独立运行全屏绘制剖析，每轮预热5秒、测量60秒，默认三轮。
.\run-resident-diagnostics.ps1 -ProfileFullscreen
```

`-Scenarios`默认仍是原来的七类；`-Rounds 1`可用于快速排查。A/B结果保存在同一目录的baseline和candidate子目录，比较文件位于candidate/comparison.json。缺失指标、不同场景集合或不完整报告不会被视作有效的同场景比较。查询次数及每秒调用率是主要验收数据；5秒CPU计时可能较粗，不设置固定CPU降幅门槛。

剖析使用独立的`resident-profile-v1`协议，不允许与A/B开关组合，也不能和普通resident-v1报告比较。每轮JSON中的profile.windows按窗口位置、尺寸与DPI分组，记录各阶段原始耗时；summary.json汇总次数、总耗时、中位数、p95。paint.total、paint.prepare/backdrop.prepare/backdrop.build以及background.restore/backdrop.copy等存在父子关系，不能相加。rest.tick为宿主状态更新；分段是墙钟耗时，不是CPU占比。只统计预热后的稳态；没有发生的阶段（例如缓存重建）不会出现。正式应用不包含这些详细计时探针。

可使用`scripts/package-ui-diagnostics.ps1 -ResidentBaseline PATH`将指定基线加入包中。应在构建新版前保存旧EXE，并核对其SHA256与原manifest一致。2026-11-11的本地已验证基线为`B1DEE704CB91CF7836FF7CED1C3DA611C65BBDD45DC3C03FD643738F5BA4F12E`；CI未显式传入时不自动捆绑旧基线。

## 第三轮：汇总恢复与下划线缓存

本轮新增 `-SummarizeDirectory`，支持单场景汇总及仅恢复已有数据。它与其它参数互斥，不需要被测 EXE；从 manifest 自动识别协议。恢复前验证轮次、成功退出日志、EXE 哈希、环境、资源采样和阶段数据，保留原始报告与日志，将原 manifest 保存为 `manifest.before-summary.json`（已有备份不覆盖）。摘要写入成功后才标记完整。比较仍拒绝未恢复的不完整目录。

以下命令均在解压目录执行；执行策略仅对启动的 PowerShell 进程生效。

```powershell
# 恢复本次已有报告，不重新采样；目录也可使用完整路径。
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\run-resident-diagnostics.ps1 -SummarizeDirectory .\resident-results-20261111-032810-19508
# 脚本回归自检；也应使用 pwsh 再运行一次。
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\run-resident-diagnostics.ps1 -SelfTest
# 128 个休憩像素案例，需交互式 Windows 桌面。要求 RGB 完全一致。
.\dunyu-ui-tests.exe --exact desktop::rest_ui::tests::cached_rest_matches_original --ignored --test-threads=1 --nocapture
# 同机旧版/新版全屏普通模式三轮交替 A/B。
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\run-resident-diagnostics.ps1 -Scenarios fullscreen -BaselineExecutable .\dunyu-resident-baseline.exe
# 新版三轮细分剖析，独立于普通 A/B。
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\run-resident-diagnostics.ps1 -ProfileFullscreen
```

下划线缓存从原设备坐标绘制结果截取，逐窗口保存小块背景与笔触合成像素；锦鲤接近时回退原绘制，尺寸、DPI或主题变化后重新建立。`stroke` 为总耗时，其子阶段为 `stroke.draw`、`stroke.build`、`stroke.copy`。锦鲤新增 `koi.transform`、`koi.lookup`、`koi.draw`、`koi.restore`；transform 仅表示矩阵设置，实际旋转、缩放和取样的栅格成本仍包含在 draw 中。父子阶段不能相加。素材、轨迹、插值算法和刷新周期保持原配置。

第三轮对照 EXE 的 SHA256 为 `51937EE375217C78A71746679FE915FF09AFF5127D4F9A04DE6C7ACC6E64D048`。交付包用于验证；Windows 像素测试及同机性能结果通过前，不将交叉编译成功视为视觉或性能验收。

## 第四轮：真实窗口缓存验收

使用 `DunYu-resident-phase4.zip`。本包以第三轮 EXE（SHA256 `91E01A0E8946AD9B8425E4E29E25913EC708BA27ABCB15C52957F26FE2047160`）作为 `dunyu-resident-baseline.exe`。继续按上面的脚本自检、像素检查、三轮全屏 A/B、细分剖析顺序运行。

下划线缓存现在在动态内容绘制前主动从静态背景建立，保持原设备坐标。构建后恢复后台背景；实际提交仍使用原重绘区域与 Windows 子窗口裁剪。鱼与下划线重叠时仍回退原绘制。`stroke.build` 属于 `paint.prepare` 内的构建耗时；`stroke` 只包含图层位置上的复制、绘制及判断，不能再将 build 当作 stroke 的子阶段相加。

原128个像素案例保留，更新主动构建后的路径计数期望。同一像素命令还会在每个实际显示器上运行8个可见窗口案例，调用真实 WM_PAINT，覆盖局部重绘、命中、重叠、主题重建、构建/复制失败回退及关闭动画；通过屏幕像素与参考路径比较 RGB，不使用 WM_PRINTCLIENT 代替真实绘制。双屏通常合计144例。保持桌面解锁、无遮挡，像素测试会短暂覆盖屏幕。

每轮报告新增可选 `stroke_cache`，包含按窗口分组的 `warmup` 和 `measurement`。各窗口记录 `applicable`、`ready`、成功 `builds` / `hits`、原绘制 `draws`、`overlap_fallbacks`、`build_failures`、`copy_failures`。剖析模式还记录每种条件首个 `examples` 样本（消息类型、实际重绘区、笔触块、鱼包围盒、安全条件与缓存状态）；普通 A/B 仅记计数。预热阶段建立缓存后，测量阶段的 builds=0 是正常情况，验收依据是测量期间 hits>0。

正常主题、开启动画且几何条件支持缓存时，每窗须有成功构建、测量命中、有效缓存且两个阶段都没有构建/复制错误。失败会先保留原始报告，再以非零退出码结束；外部资源样本保存在 `.samples.json`。失败目录不参与成功基线比较。高对比度、关闭动画或不支持的几何条件标为 `not_applicable`；旧报告缺少新字段标为 `not_reported`。这些状态不等于缓存优化通过。

Windows PowerShell 5.1及PowerShell 7自检、可见窗口像素比较和新包的实际命中仍需Windows执行；交叉编译成功不等于这些运行验收已通过。CPU收益须由本次三轮 A/B 与命中数据一起确认。
