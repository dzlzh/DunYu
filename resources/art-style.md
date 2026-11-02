# 遁鱼视觉风格

本次视觉以 `docs/design` 顶层设计稿为准，`bak` 保存历史备选。采用深青纸纹、米白文字、朱砂强调色、水墨图标与笔触装饰。

- `fish.png` 与 `dunyu.ico` 来自 `app-icon-loop`，统一应用、托盘和页头标识。
- `nav-icons-ink.png` 为六枚导航图标设计原图；绘制时只取图形区域并去除深色底。
- `koi-natural-frames.png` 是自然游动透明 WebP 的完整帧 PNG 图集，160×160 单元纵向排列，共50帧，每帧40毫秒。生成时先合成增量帧，再等比缩放；运行时无需额外解码器。
- 背景使用从设计稿制作的 `paper-teal.png`，按窗口尺寸等比覆盖并缓存。表单使用安静的深青底，主操作为朱砂红配浅色文字。
- 中文标题与固定文案内嵌思源宋体 Regular/SemiBold，输入框使用微软雅黑 UI，倒计时使用 Segoe UI Light。高对比模式使用系统颜色，关闭装饰纹理；减少动画时使用静态锦鲤。

旧图标和分层动画提示词仅记录历史生成过程，不再作为当前视觉约束。

图集可用 ImageMagick 从设计源文件重新生成：

```sh
convert docs/design/koi-swim-natural-transparent.webp -coalesce -resize 160x160 -append resources/koi-natural-frames.png
```

## 本轮背景与圆环素材

使用内置 imagegen，以 `docs/design/rest-preview-fullscreen.png` 为编辑目标，输出保存为 `resources/paper-teal.png` 和 `resources/ink-ring.png`。原设计保留。

背景提示词：保留原图深青纸纹、色调与右上和左下的淡色笔触；移除全部文字、计时、中心圆环、鱼、图标和按钮，填补为连续纸纹，不添加新元素。

圆环提示词：提取中央米白手绘圆环为正方形透明 PNG；保留上细下粗的不规则干笔纹理；去除朱砂进度、鱼、文字、背景和角落笔触，环内外透明。

字体来自 Adobe 官方 Source Han Serif release 分支的简体中文 OTF；完整字体未经修改，许可证见 `fonts/OFL.txt`。
