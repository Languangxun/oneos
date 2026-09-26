# OneOS

可启动的模拟操作系统（教学 / 实验用途）。内核与基础系统复用 Debian，用户态自己写：

- `oneosd`：会话守护进程，systemd socket 激活 + JSON Lines 协议
- `oneos`：命令行客户端（状态 / 服务 / 日志 / 设置 / 会话 / 电源）
- 桌面：Wayland（labwc + waybar + fuzzel + swaybg + foot）
- 构建：mkosi 声明式产出可启动 UEFI 磁盘镜像（systemd-boot + UKI）

## 快速开始

主机要求：Ubuntu / Debian、Rust（rustup）、mkosi 26+、QEMU。

```sh
make deps      # 安装 mkosi / qemu 等宿主依赖（需要 sudo）
make image     # 编译 Rust 组件并构建可启动磁盘镜像
make run       # 用 QEMU 启动（SDL 窗口）
```

启动后自动以 root 登录（仅供开发，无密码）。进入桌面：

```sh
oneos session start
```

桌面快捷键：

| 快捷键 | 功能 |
|---|---|
| `Super+Enter` | 打开终端（foot） |
| `Super+D` | 打开启动器（fuzzel） |
| `Super+E` | 系统设置面板（`oneos-settings`） |
| `Super+Q` | 关闭当前窗口 |
| `Super+Shift+E` | 退出桌面 |

> Ubuntu 默认的 AppArmor 限制会阻止非特权 userns，因此 mkosi 通过 `sudo` 运行。
> 若你已放开该限制，可用 `make image SUDO= MKOSI=mkosi` 以普通用户构建。

## 命令一览

```sh
oneos status                          # 系统状态
oneos session status|start|stop       # 图形会话
oneos service list                    # 服务列表
oneos service status|start|stop|restart <unit>
oneos logs [-u unit] [-n lines]       # 日志（journal）
oneos settings [show|hostname <name>|timezone <zone>]
oneos poweroff | reboot

oneos-settings                        # 设置面板（whiptail 对话框）
```

没有显示环境时用串口控制台启动：`make run-serial`。

## 原理教程

### 1. 分层

```
┌─────────────────────────────────────────────────────┐
│ 应用（规划中）                                        │
├─────────────────────────────────────────────────────┤
│ 桌面：labwc（合成器/WM）+ waybar + fuzzel + foot      │
├─────────────────────────────────────────────────────┤
│ 会话：oneos-session.service（由 oneos session 控制）  │
├─────────────────────────────────────────────────────┤
│ 系统服务：oneosd（unix socket JSON API） / oneos CLI  │
├─────────────────────────────────────────────────────┤
│ 基础系统：systemd、networkd、resolved、dbus、sshd      │
├─────────────────────────────────────────────────────┤
│ 内核：Debian linux-image-amd64                        │
├─────────────────────────────────────────────────────┤
│ 引导：UEFI → systemd-boot → UKI（内核+initrd+cmdline）│
└─────────────────────────────────────────────────────┘
```

设计原则：**只做用户态**。内核、驱动、包管理全部复用 Debian，
系统能力（进程、cgroup、日志、沙箱、socket 激活）全部复用 systemd。

### 2. 镜像构建：mkosi 做了什么

`mkosi.conf` 是声明式配置：

- `[Distribution]`：用 Debian trixie（阿里云镜像）
- `[Output]`：`Format=disk` 产出 GPT 磁盘镜像
- `[Content]`：要安装的包、`Autologin=yes`、内核命令行
- `[Runtime]`：`mkosi vm` 启动 QEMU 时的参数（`Console=gui`、分辨率、虚拟键鼠）

构建流程大致是：

1. 从软件源安装包到临时根目录
2. 把 `mkosi.extra/` 原样覆盖进根目录（我们的品牌、systemd 单元、桌面配置、二进制）
3. 生成 initrd、组装 **UKI**（把内核、initrd、cmdline、os-release 打包成一个 `.efi`）
4. 用 `systemd-repart` 写出磁盘镜像：一个 ESP（512M）+ 一个 ext4 根分区

内核命令行里的 `root=PARTUUID` 是占位符，mkosi 会替换成实际根分区 UUID。
所以 `mkosi.extra/` 就是"配置即代码"：改 `/etc`、写 systemd 单元、放用户配置都在这里。

### 3. 启动链路

```
OVMF 固件 → ESP 上的 systemd-boot → EFI/Linux/oneos-*.efi（UKI）
   → 内核 + initrd → systemd（PID 1）
   → sysinit → basic → multi-user → graphical target
   → oneosd.socket 开始监听 /run/oneos/oneosd.sock
   → oneos-splash 在 getty 前播放连笔 OneOS 开机动画（写 /dev/fb0）
   → oneos-boot-record 在 multi-user.target 之后记录本次启动耗时，供下次预估
   → getty 自动登录 root（tty1 / tty0 / hvc0）
```

### 4. oneosd 与通信协议

systemd 先创建 socket 并监听，**第一个连接到来时才启动 `oneosd`**
（socket 激活）。`oneosd` 从环境变量 `LISTEN_FDS`/`LISTEN_PID` 得知自己继承了
fd 3，直接在上面 accept；本机开发时没有 systemd，就自己 bind。

协议是一行一个 JSON（JSON Lines）：

```json
// 请求
{"id":1,"method":"status"}
{"id":2,"method":"service_status","params":{"unit":"oneosd.service"}}
```

```json
// 成功
{"id":1,"ok":true,"result":{"version":"0.0.6","hostname":"oneos", "...":"..."}}
// 失败
{"id":2,"ok":false,"error":{"code":"unit_not_found","message":"nope.service: unit not found"}}
```

类型定义在 `crates/oneos-proto`。写操作（电源、服务启停、设置修改）在开发模式
（`ONEO_DEV=1`）下被拒绝，避免误操作宿主机。

### 5. 图形桌面是怎么跑起来的
Wayland 模型：

- 内核的 **DRM/KMS** 管显示输出，**libinput/evdev** 管输入
- **合成器（compositor）就是显示服务器**：它持有 DRM master、接收输入、
  管理窗口，客户端（终端、面板）把渲染结果作为缓冲区提交给它
- 与 X11 不同，没有全局屏幕坐标和全局事件队列，一切由合成器仲裁

本项目里各组件分工：

| 组件 | 角色 |
|---|---|
| labwc | 合成器 + 堆叠窗口管理（标题栏、快捷键、工作区） |
| waybar | 顶栏：工作区、时钟、网络、CPU、内存 |
| fuzzel | 启动器（`Super+D`） |
| swaybg | 壁纸 |
| foot | 终端 |

配置放在 `/root/.config/{labwc,waybar,fuzzel,foot}`，随 `mkosi.extra/` 进镜像。

几个关键点（都是踩坑换来的）：

- QEMU 的 virtio-vga 没有 3D，合成器用 `WLR_RENDERER=pixman` 纯软件渲染
- 没有 logind 会话，`LIBSEAT_BACKEND=builtin` 让 libseat 直接拿 DRM/输入设备
- 必须等 udev 枚举完输入设备再启动，否则 `libinput: no input devices`，
  合成器会"启动失败但抢走了显示"，表现为画面冻结（所以单元里
  `Wants/After=systemd-udev-settle.service`）
- `oneos-session.service` 用 `Conflicts=getty@tty1.service` 占用 tty1

`oneos session start/stop/status` 本质上就是 `systemctl start/stop/is-active oneos-session.service`。

### 6. 开机动画是怎么做的

![开机动画](docs/boot-animation.png)

仿 Apple "Hello"（参考 MIT 项目 [InkTrail](https://github.com/GXeLla/InkTrail)）：
不是描字的外轮廓，而是**保留字体的填充字形，用一根粗"墨迹"沿轮廓逐步显影**，
所以看起来像墨水在纸上写字。OneOS 的做法：

1. `tools/gen-signature.py` 用 fontTools 读取手写字体 **Caveat**（OFL），
   把 "OneOS" 的字形展平成闭合轮廓，写入 `crates/oneos-splash/src/signature.data`
2. `oneos-splash`（零依赖 Rust）启动时把轮廓超采样填充成一张静态掩码；
   每帧按"已写弧长"把轮廓画成粗笔画得到墨迹掩码，两者相乘再写到 `/dev/fb0`
3. 每字母 0.5s 顺序书写，缓动 `cubic-bezier(0.4,0,0.2,1)`，下方配细进度条
4. `oneos-splash.service` 在 `getty.target` 之前运行，放完动画才出现登录提示
5. **动画时长自适应**：`oneos-boot-record.service` 每次启动把"到
   multi-user.target 的耗时"追加进 `/var/lib/oneos/boot-history`（保留最近 8 次）。下次启动时
   `oneos-splash` 取最近几次的均值，减去当前 uptime 再乘 0.8（留余量），
   得到本次可用的播放时长；系统启动越快，动画播放越快（最快 0.9s），
   保证每次都在登录提示出现前刚好放完，不再拖慢启动

完整原理、参数调整、本地预览方法见 [docs/BOOT-ANIMATION.md](docs/BOOT-ANIMATION.md)。

### 7. 服务与设置是怎么实现的

- `oneos service list`：调用 `systemctl list-units --output=json`，把 JSON 转成
  我们的 `ServiceInfo` 结构返回；启停前会校验单元名（拒绝 `-` 开头、空白、`/` 等），
  并检查 `LoadState` 是否存在
- `oneos settings`：读取 `/etc/hostname`、`/etc/localtime` 链接、`/etc/locale.conf`；
  修改时先做严格校验（主机名 RFC 风格、时区必须存在于 `/usr/share/zoneinfo`），
  再调用 `hostnamectl` / `timedatectl`
- `oneos-settings`：whiptail 对话框面板（系统信息 / 主机名 / 时区 / 服务 / 日志 /
  电源），通过 `oneos` CLI 走 oneosd API，因此校验规则与命令行完全一致；
  从桌面按 `Super+E`、右键菜单或 fuzzel 搜索 "OneOS 设置" 都能打开

### 8. 开发循环

```sh
make dev                     # 本机跑 oneosd（dev 模式，读写分离），执行 oneos status
make dev ARGS="service list" # 换命令
make debug                   # QEMU：串口控制台 + virtio-gpu（调桌面/会话）
make run-serial              # 纯串口控制台
make ssh                     # VSock 连入正在运行的虚拟机
make shell                   # 以容器方式进入镜像根文件系统
make lint fmt test           # clippy / rustfmt / 单测
```

改 Rust 代码 → `make dev` 秒级验证；改镜像内容（包、单元、配置）→ `make image` 重建。
GitHub Actions 会在 push 时自动跑 fmt / clippy / test。

## 目录结构

```
mkosi.conf                    镜像构建配置
mkosi.extra/                  覆盖进镜像根目录（品牌/单元/桌面配置/二进制）
mkosi.extra/usr/bin/oneos-settings   设置面板（whiptail shell 脚本）
mkosi.extra/usr/share/applications/  fuzzel 启动器条目
crates/oneos-proto/           协议定义与客户端库
crates/oneosd/                守护进程
crates/oneos/                 命令行客户端
crates/oneos-splash/          开机动画 + 启动耗时记录（fbdev，零依赖）
tools/gen-signature.py        连笔路径生成器（Hershey 字体）
scripts/dev.sh                本机开发脚本
docs/ARCHITECTURE.md          架构细节
docs/BOOT-ANIMATION.md        开机动画原理
docs/ROADMAP.md               路线图
```

## 排错

- **桌面没起来 / 画面冻结**：`make debug` 进串口，看
  `systemctl status oneos-session` 和 `journalctl -b -u oneos-session`
- **开机动画没出现**：确认 `ls /dev/fb0` 存在、`systemctl status oneos-splash`；
  串口模式（无 virtio-vga）会按条件跳过，这是预期行为
- **QEMU 窗口没弹出来**：`make run-serial` 对照；GUI 模式依赖主机 PipeWire，
  Makefile 已传 `PIPEWIRE_RUNTIME_DIR`
- **分辨率**：改 `mkosi.conf` 的 `[Runtime] KernelCommandLineExtra=video=1920x1080`
  （运行时参数，不用重建；要固定进镜像才重建）
- **构建权限**：Ubuntu AppArmor 限制非特权 userns，mkosi 需要 `sudo`
  （或 `sudo sysctl kernel.apparmor_restrict_unprivileged_userns=0` 后 `SUDO=`）
- **清理产物**：`sudo rm -rf mkosi.output`（root 属主）
