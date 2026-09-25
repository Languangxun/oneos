# 架构

## 分层

| 层 | 组件 | 状态 |
|---|---|---|
| 应用 | OneOS 应用 | 规划中 |
| 图形会话 | oneos-session（cage 合成器 + foot） | 已有 |
| 系统服务 | `oneosd`（unix socket JSON API） | 已有 |
| 基础系统 | systemd、networkd、resolved、dbus、openssh | 已有 |
| 内核 | Debian linux-image-amd64 | 已有 |
| 构建 | mkosi（声明式，产出 GPT 磁盘 + UKI） | 已有 |

设计原则：内核与驱动全部复用 Linux/Debian，只在用户态做自己的东西。

## 仓库布局

```
mkosi.conf              镜像构建配置
mkosi.extra/            原样覆盖到镜像根目录
  etc/                  品牌、网络、resolv.conf、systemd 启用软链
  usr/lib/systemd/system/  oneosd.socket / oneosd.service
  usr/bin/              make build 放入静态编译的 oneosd / oneos
crates/oneos-proto/     协议与客户端库
crates/oneosd/          守护进程（socket 激活）
crates/oneos/           命令行客户端
scripts/dev.sh         本机开发脚本（不启动虚拟机）
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
```

成功响应：

```json
{"id":1,"ok":true,"result":{"version":"0.1.0","os":"OneOS 0.1.0","hostname":"oneos","uptime_secs":42,"boot_id":"..."}}
```

失败响应：

```json
{"id":1,"ok":false,"error":{"code":"dev_mode","message":"power operations are disabled in dev mode"}}
```

当前 method：`ping`、`status`、`poweroff`、`reboot`、`session_status`、`session_start`、`session_stop`、
`service_list`、`service_status`、`service_start`、`service_stop`、`service_restart`、`logs`。

其中 `service_*` 与 `session_*` 属于写操作，开发模式（`ONEO_DEV=1`）下会被拒绝；
`service_list`、`service_status`、`logs` 是只读操作，开发模式下直接作用于宿主机 systemd/journal，便于调试。

## 运行方式

- 镜像内：`oneosd.socket` 由 systemd 监听，首个连接到来时自动拉起 `oneosd.service`
- 本机开发：`make dev`，`ONEO_SOCKET=/tmp/oneos-dev.sock` + `ONEO_DEV=1`
  （开发模式会禁用 poweroff/reboot，避免把宿主机电源关掉）

## 图形会话

`oneos-session.service` 是图形会话单元，由 `oneos session start` 启动，运行
`cage`（Wayland kiosk 合成器）+ `foot`（终端）。运行在无 logind 会话的环境下，
因此用 `LIBSEAT_BACKEND=builtin` 直接访问 DRM/evdev，用 `WLR_RENDERER=pixman`
做纯软件渲染（QEMU 的 virtio-vga 没有 3D）。后续替换为自研合成器（smithay）。
