# 路线图

## v0.0.1 框架（当前）

- [x] mkosi 声明式构建可启动 Debian 镜像（systemd-boot + UKI）
- [x] oneosd 守护进程（socket 激活 + JSON 协议）
- [x] oneos CLI（status / ping / poweroff / reboot）
- [x] OneOS 品牌（os-release / motd / issue）
- [x] 网络（systemd-networkd）与 VSock SSH

## v0.0.2 图形会话

- [x] oneos-session：由 systemd 启动图形会话（`oneos session start`）
- [x] Wayland 合成器（cage 起步）
- [x] 首个窗口应用（foot 终端）
- [x] oneosd 会话 API（status / start / stop）
- [x] 中文字体（fonts-noto-cjk）
- [ ] 输入法（依赖合成器实现 input-method 协议，随自研合成器推进）
- [ ] 自研合成器（基于 smithay）

## v0.0.3 系统管理

- [x] `oneos service`（list / status / start / stop / restart）
- [x] `oneos logs`（journal 转发）
- [x] `oneos settings`（show / hostname / timezone）
- [ ] 图形设置面板

## v0.0.4 应用与差异化

- [ ] 应用打包与分发（portable service 或自有格式）
- [ ] 快照与回滚（btrfs）
- [ ] 不可变 /usr 与原子更新

## 待定

- 真机安装镜像（UEFI / BIOS）
- 多用户与权限模型
