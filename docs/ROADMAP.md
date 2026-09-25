# 路线图

## v0.0.1 框架（已完成）

- [x] mkosi 声明式构建可启动 Debian 镜像（systemd-boot + UKI）
- [x] oneosd 守护进程（socket 激活 + JSON 协议）
- [x] oneos CLI（status / ping / poweroff / reboot）
- [x] OneOS 品牌（os-release / motd / issue）
- [x] 网络（systemd-networkd）与 VSock SSH

## v0.0.2 图形会话（已完成）

- [x] oneos-session：由 systemd 启动图形会话（`oneos session start`）
- [x] Wayland 合成器起步（cage）
- [x] 首个窗口应用（foot 终端）
- [x] oneosd 会话 API（status / start / stop）
- [x] 中文字体（fonts-noto-cjk）
- [x] 修复启动时序：等待 systemd-udev-settle，避免输入设备未就绪导致 cage 崩溃

## v0.0.3 系统管理（已完成）

- [x] `oneos service`（list / status / start / stop / restart）
- [x] `oneos logs`（journal 转发）
- [x] `oneos settings`（show / hostname / timezone）

## v0.0.4 设置与 CI（已完成）

- [x] GitHub Actions：fmt / clippy / test

## v0.0.5 桌面环境（当前）

- [x] labwc 合成器（窗口管理、标题栏、快捷键）
- [x] waybar 状态栏（workspaces / 时钟 / 网络 / CPU / 内存）
- [x] fuzzel 启动器、swaybg 壁纸、foot 主题
- [x] OneOS 深色主题与键位（labwc menu.xml + themerc）
- [x] Apple Hello 风开机动画（Caveat 填充字形 + 墨迹遮罩，仿 InkTrail）
- [ ] 图形设置面板
- [ ] 自研合成器（以 smithay 起步，替代 labwc）
- [ ] 输入法（需要合成器实现 input-method 协议）

## 后续

- [ ] 应用打包与分发（portable service 或自有格式）
- [ ] 快照与回滚（btrfs）
- [ ] 不可变 /usr 与原子更新
- [ ] 真机安装镜像（UEFI / BIOS）
- [ ] 多用户与权限模型
