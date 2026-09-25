# 路线图

## v0.0.1 框架（当前）

- [x] mkosi 声明式构建可启动 Debian 镜像（systemd-boot + UKI）
- [x] oneosd 守护进程（socket 激活 + JSON 协议）
- [x] oneos CLI（status / ping / poweroff / reboot）
- [x] OneOS 品牌（os-release / motd / issue）
- [x] 网络（systemd-networkd）与 VSock SSH

## v0.0.2 图形会话

- [ ] oneos-session：登录后启动图形会话（基于 systemd-logind）
- [ ] Wayland 合成器（起步用 cage，长期基于 smithay 自研）
- [ ] 首个窗口应用（终端）
- [ ] oneosd 增加会话 API（list / start / stop）
- [ ] 镜像加入 GPU/输入法所需固件与 mesa

## v0.0.3 系统管理

- [ ] `oneos service`（list / start / stop / status）
- [ ] `oneos logs`（转发 journal）
- [ ] 系统设置面板

## v0.0.4 应用与差异化

- [ ] 应用打包与分发（portable service 或自有格式）
- [ ] 快照与回滚（btrfs）
- [ ] 不可变 /usr 与原子更新

## 待定

- 真机安装镜像（UEFI / BIOS）
- 多用户与权限模型
