# 架构

## 分层

| 层 | 组件 | 状态 |
|---|---|---|
| 应用 | OneOS 应用 | 规划中 |
| 桌面 | labwc / waybar / fuzzel / swaybg / foot | 已有 |
| 会话 | oneos-session.service、`oneos session` API | 已有 |
| 系统服务 | `oneosd`（unix socket JSON API） | 已有 |
| 基础系统 | systemd、networkd、resolved、dbus、openssh | 已有 |
| 内核 | Debian linux-image-amd64 | 已有 |
| 构建 | mkosi（声明式，产出 GPT 磁盘 + UKI） | 已有 |

设计原则：内核与驱动全部复用 Linux/Debian，只在用户态做自己的东西。

## 仓库布局

```
mkosi.conf                     镜像构建配置（发行版/包/引导/运行时）
mkosi.extra/                   原样覆盖到镜像根目录
  etc/                         品牌、网络、resolv.conf、systemd 启用软链
  usr/lib/systemd/system/      oneosd.socket / oneosd.service / oneos-session.service
  usr/bin/                     make build 放入静态编译的 oneosd / oneos / oneos-splash
  root/.config/                桌面配置（labwc / waybar / fuzzel / foot）
crates/oneos-proto/            协议定义与客户端库
crates/oneosd/                 守护进程（socket 激活）
crates/oneos/                  命令行客户端
crates/oneos-splash/           开机动画（fbdev，零依赖）
tools/gen-signature.py         用 fontTools + Caveat 生成字形轮廓数据
tools/fonts/Caveat.ttf         手写字体（OFL，含许可证）
scripts/dev.sh                 本机开发脚本（不启动虚拟机）
docs/                          架构、路线图、开机动画原理
```

## 为什么是 Debian + mkosi

- Debian 稳定、包最全，`ID_LIKE=debian` 后续可复用整个生态
- mkosi 用声明式配置直接产出可启动磁盘镜像（UEFI + systemd-boot + UKI），
  `mkosi vm` 一条命令进 QEMU，`mkosi ssh` 通过 VSock 连入
- systemd 提供进程管理、cgroup、沙箱、日志等系统原语，不需要自己造

## 通信协议

`oneos` 与 `oneosd` 通过 `/run/oneos/oneosd.sock` 通信，一行一个 JSON（JSON Lines）。
类型定义在 `crates/oneos-proto`。

请求：

```json
{"id":1,"method":"status"}
{"id":2,"method":"service_status","params":{"unit":"oneosd.service"}}
```

成功响应：

```json
{"id":1,"ok":true,"result":{"version":"0.0.5","os":"OneOS 0.0.5","hostname":"oneos","uptime_secs":42,"boot_id":"..."}}
```

失败响应：

```json
{"id":1,"ok":false,"error":{"code":"dev_mode","message":"power operations are disabled in dev mode"}}
```

当前 method：`ping`、`status`、`poweroff`、`reboot`、`session_status`、`session_start`、`session_stop`、
`service_list`、`service_status`、`service_start`、`service_stop`、`service_restart`、`logs`、
`settings_show`、`settings_set_hostname`、`settings_set_timezone`。

写操作（`poweroff`、`reboot`、`session_start/stop`、`service_start/stop/restart`、`settings_set_*`）
在开发模式（`ONEO_DEV=1`）下会被拒绝；只读操作（`status`、`service_list`、`service_status`、`logs`、
`settings_show`）在开发模式下直接作用于宿主机，便于调试。

## 运行方式

- 镜像内：`oneosd.socket` 由 systemd 监听，首个连接到来时自动拉起 `oneosd.service`
- 本机开发：`make dev`，`ONEO_SOCKET=/tmp/oneos-dev.sock` + `ONEO_DEV=1`

## 桌面会话

`oneos-session.service` 由 `oneos session start` 启动，运行 `labwc`：

- **labwc**：wlroots 合成器 + 堆叠式窗口管理（标题栏、快捷键、工作区）
- **waybar / swaybg / fuzzel / foot**：状态栏、壁纸、启动器、终端，由 labwc 的 autostart 拉起
- 配置位于 `/root/.config/{labwc,waybar,fuzzel,foot}`，随 `mkosi.extra/` 进入镜像

无 logind 会话下运行，因此：

- `LIBSEAT_BACKEND=builtin`：libseat 直接访问 DRM master 与输入设备
- `WLR_RENDERER=pixman`：QEMU 的 virtio-vga 没有 3D，用纯软件渲染
- `Wants/After=systemd-udev-settle.service`：等 udev 枚举完输入设备再启动，
  避免 `libinput: no input devices` 导致合成器启动失败
- `Conflicts=getty@tty1.service`：图形会话占用 tty1

## 构建产物

`mkosi` 产出 `mkosi.output/oneos_<version>.raw`：

- ESP（512M，systemd-boot + `EFI/Linux/oneos-*.efi` 统一内核镜像）
- 根分区（ext4，按内容 Minimize，安装后约 1G）

`oneosd` / `oneos` / `oneos-splash` 在本机用 musl 静态编译（`make build`），
复制进 `mkosi.extra/usr/bin/`，因此镜像内不依赖任何运行时库版本。

## 开机动画

`oneos-splash` 在 getty 之前把 "OneOS" 写到 `/dev/fb0`：字形是 Caveat（OFL）的
真实填充轮廓（`tools/gen-signature.py` 离线生成），播放时用粗墨迹沿轮廓显影，
仿 Apple Hello / InkTrail 的效果。播放器零依赖、支持 16/32bpp framebuffer。
详见 [BOOT-ANIMATION.md](BOOT-ANIMATION.md)。
