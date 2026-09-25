# OneOS

可启动的模拟操作系统（教学 / 实验用途）。基于 Debian + systemd + mkosi 构建，
自带会话守护进程 `oneosd` 与命令行工具 `oneos`，后续将加入图形会话与窗口。

## 环境要求

- 主机：Ubuntu / Debian
- Rust（rustup，需 `x86_64-unknown-linux-musl` target）
- mkosi 26+ 与 QEMU（`make deps` 一键安装，需要 sudo）

## 快速开始

```sh
make deps      # 安装 mkosi / qemu 等宿主依赖
make genkey    # 首次可选：生成 mkosi SSH 密钥
make image     # 编译 Rust 组件并构建可启动磁盘镜像
make run       # 用 QEMU 启动
```

> Ubuntu 默认的 AppArmor 限制会阻止非特权 userns，因此 mkosi 通过 `sudo` 运行。
> 若你已放开该限制，可用 `make image MKOSI=mkosi` 以普通用户构建。

镜像启动后会在 tty1 自动以 root 登录（仅供开发，无密码，请勿暴露到网络）。

## 日常开发

```sh
make dev           # 不开虚拟机：本机运行 oneosd 并执行 oneos status
make dev ARGS=ping # 换一个命令
make ssh           # VSock 连入正在运行的虚拟机
make shell         # 以容器方式进入镜像根文件系统
make lint fmt test # 代码检查 / 格式化 / 测试
```

## 目录结构

```
mkosi.conf         镜像构建配置（Debian trixie + systemd-boot + UKI）
mkosi.extra/       直接覆盖进镜像根目录的文件（品牌、unit、网络、二进制）
crates/oneos-proto 协议定义（JSON over unix socket）
crates/oneosd      会话守护进程
crates/oneos       命令行客户端
scripts/dev.sh     本机无虚拟机开发脚本
docs/              架构与路线图
```

## 文档

- [架构说明](docs/ARCHITECTURE.md)
- [路线图](docs/ROADMAP.md)
