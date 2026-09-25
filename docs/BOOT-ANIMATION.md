# 开机动画原理

![预览](boot-animation.png)

目标：黑底、白色连笔 "OneOS" 逐字写出、下方一条细进度条，苹果 Hello 动画的感觉。

## 参考

- [InkTrail](https://github.com/GXeLla/InkTrail)（MIT）：把任意文字变成自带动画 SVG，
  核心是 **保留字体的填充字形，用一个粗描边遮罩沿字形轮廓逐步显露**，
  所以看起来像墨水在写字，而不是描轮廓。
- Apple 的 "Hello" 演示用的字体是手写体 **Caveat**，InkTrail 的 Apple-like 预设
  就是 `fontId: caveat`、每字母 0.42s、顺序书写、描边宽度比 0.13、圆头、softness 0.5。
- 我们照这个思路实现了一个不依赖 SVG/浏览器的 **fbdev 播放器**。

## 素材与数据流

```
tools/fonts/Caveat.ttf          # OFL 授权手写字体（含 OFL-Caveat.txt）
        │  fontTools (TrueType 轮廓 -> 展平的闭合轮廓)
        ▼
tools/gen-signature.py
        │  文本排版 -> 归一化坐标（y 向下，最长边=1）
        │  每行 "C x y x y ..." 是一条闭合轮廓，G 行分组到字母
        ▼
crates/oneos-splash/src/signature.data
        │  include_str! 编进二进制
        ▼
oneos-splash                    # 每帧把「填充 ∧ 墨迹」画到 /dev/fb0
```

重新生成（换文字/字体/字号）：

```sh
pip install --user fonttools
python3 tools/gen-signature.py --text OneOS --font tools/fonts/Caveat.ttf
cargo build -p oneos-splash
```

## 渲染原理

播放器里有两张掩码：

1. **填充掩码（静态）**：用非零环绕数的扫描线算法把闭合轮廓填成覆盖率图。
   坐标先放大 4 倍超采样，再降采样回屏幕分辨率，得到抗锯齿边缘。
   文字固定不变，所以只算一次。
2. **墨迹掩码（每帧）**：把每条轮廓按"已写弧长"画成一根很粗的圆头笔画
   （宽度 = 字号 × 0.13），边缘按 softness 做柔和衰减。
3. 合成：`像素 = 填充覆盖率 × 墨迹覆盖率 × 白色`——只有"笔走过"的区域才显露字形，
   和 InkTrail 的 SVG mask + `stroke-dashoffset` 是同一个数学。

时间轴（仿其 Apple-like 预设）：

- 每字母 0.5s，字母之间间隔 0.06s，顺序书写（上一个写完再下一个）
- 缓动 `cubic-bezier(0.4, 0, 0.2, 1)`（smooth），用二分法解曲线
- 5 个字母总时长约 2.9s，末尾停留 1s
- 进度条按总时长线性填充

开机时序由 `oneos-splash.service` 控制：`Before=getty.target`（挡住登录），
`ConditionPathExists=/dev/fb0`（纯串口模式自动跳过）。内核命令行加了
`quiet` 和 `vt.global_cursor_default=0`。

## 本地预览

不用进虚拟机：

```sh
cargo run -p oneos-splash -- --preview /tmp/oneos-splash
# 生成 frame-00.ppm ... frame-08.ppm（P6），可用 ffmpeg/Pillow 合成 GIF 或拼图
```

## 可以改的地方

- 文字：`--text`；字体：`--font`（任意 TTF/OTF，注意授权）
- 时长、颜色、描边比例、softness、进度条位置：
  `crates/oneos-splash/src/main.rs` 顶部常量
- 想要真正的"笔锋"（粗细变化）：把生成器改成输出 SVG 轮廓 + 变宽度，
  或按 InkTrail 的思路在遮罩上叠加压力曲线
