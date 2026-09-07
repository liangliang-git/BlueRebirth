<div align="center">

# BlueOath Rebirth

**某款已关服 Unity 手游的本地离线复原工程** — 从日服/国服客户端还原出可一键启动的本地服务端与 Mod 环境。

![.NET](https://img.shields.io/badge/.NET-8.0-512BD4?style=for-the-badge&logo=dotnet)
![Lang](https://img.shields.io/badge/C%23%20%2B%20Lua-2E8B57?style=for-the-badge)
![Status](https://img.shields.io/badge/Status-Update-yellowgreen?style=for-the-badge)

</div>

---

## 项目简介

某游戏是一款基于 **Unity IL2CPP**（C# 已编译不可读）与 **Lua 热更**（逻辑近乎明文）的手游，目前已关服。本项目通过逆向还原其网络协议、配置数据与客户端逻辑，搭建一套本地离线服务端，让游戏能够**离线状态下一键运行**，并在此之上提供 **Mod 支持**（xLua 运行时执行外部 Lua 代码），目标在日服与国服客户端之间通用。

原始日服与国服客户端目录保持不变；协议、IL2CPP 类型与配置知识库均由工具**可重复生成**。

## 特性

- ✅ **本地离线服务端** — Rust：HTTP 引导 + 游戏登录（TCP 与 UDP）双端点，SQLite 存档，仅监听 `127.0.0.1`
- ✅ **真实传输协议还原** — 11 字节应用层头 + protobuf 信封；TCP over UDP（`KcpCodec` + ARQ 可靠性）
- ✅ **客户端注入与重定向** — `client-injector/` 内含 x86 注入 DLL（xinput 劫持）、SDK 登录绕过、DNS/connect/TLS 重定向、UnityTLS 证书信任补丁、引导系统跳过
- ✅ **配置数据库双向转换** — 解密 `config_*.db`（XOR 0x55）⇄ Excel，一键脚本导出/反导
- ✅ **协议/类型/配置知识库** — `BlueOath.Tools` 只读分析生成 `docs/*-catalog`，含 `.proto` 草案与 wire 证据
- ✅ **Mod 支持（实验）** — xLua Mod Loader：Payload 内钩取 `lua_pcallk`，从游戏 Lua 线程执行 `client-injector/Mods/bootstrap.lua`

## 目录

- [快速开始](#快速开始)
- [构建与测试](#构建与测试)
- [使用方式](#使用方式)
- [项目结构](#项目结构)
- [当前进度](#当前进度)
- [文档](#文档)
- [Roadmap 与更新日志](#roadmap-与更新日志)
- [免责声明](#免责声明)

## 快速开始

### 前提条件

在开始前，您必须在本项目根目录下放置某游戏的客户端。

请参见：[项目结构](#项目结构)

### 命令行脚本

```powershell
.\run-game.bat          # 全流程：server + proxy + 注入 + 看日志
.\start-rust-client-only.bat # 调试：仅注入客户端，连接已运行 Rust 服务
```

### 手动运行本地 Rust 服务

```powershell
# 固定端口，供代理转发
cargo run --manifest-path .\rust-server\Cargo.toml -p blueoath-server -- `
  --port=7080 --game-login-port=7201 --region=jp --data=.\runtime\jp `
  --client-path=.\blueoath\blueoath
```

服务端启动后，另开终端执行 `.\start-rust-client-only.bat` 注入客户端。

若客户端走 KCP 登录，Rust 使用 `--kcp-game-login-port=<port>` 开启 UDP 端点；未开启时继续使用 TCP NetSocket。

服务只监听 `127.0.0.1`，启动时输出 JSON 健康信息与实际端口。

## 构建与测试

```powershell
cargo fmt --manifest-path .\rust-server\Cargo.toml --all -- --check
cargo clippy --manifest-path .\rust-server\Cargo.toml --workspace --all-targets -- -D warnings
cargo test --manifest-path .\rust-server\Cargo.toml --workspace
dotnet restore .\BlueOath.Local.sln
dotnet build .\BlueOath.Local.sln --no-restore

# 构建原生注入组件（payload + injector）
powershell -File .\client-injector\scripts\build-native.ps1
```

## 使用方式

### 客户端配置数据库 ⇄ Excel

客户端原始配置为 SQLite `config_*.db`，`jsonbytes` 为明文 JSON 逐字节 `XOR 0x55`（可逆）；
服务端部署使用裁剪后的 `config_*.json`，不依赖 DB 文件。

```powershell
.\export-config.bat [jp|cn]    # 导出所有 config_*.db -> <仓库>\excel
.\import-config.bat [jp|cn]    # 反导回配置数据库（自动备份）
```

底层命令与完整格式说明见 [配置工具链](docs/config-catalog/tooling.zh-CN.md)。

### 协议 / 类型 / 配置知识库生成

```powershell
dotnet run --project .\src\BlueOath.Tools\BlueOath.Tools.csproj -- --analyze-il2cpp
dotnet run --project .\src\BlueOath.Tools\BlueOath.Tools.csproj -- --analyze-wire
dotnet run --project .\src\BlueOath.Tools\BlueOath.Tools.csproj -- --analyze-protocol
dotnet run --project .\src\BlueOath.Tools\BlueOath.Tools.csproj -- --analyze-config
```

### 数据驱动的 GM / 玩法配置

| 文件 | 作用 |
| --- | --- |
| `rust-server/catalog/data/gm-goods.json` | GM 商店商品（货币/道具/时装） |
| `rust-server/catalog/data/gm-mails.json` | 邮件（无限领取的货币邮件） |
| `rust-server/catalog/data/build-pools.json` | 建造卡池（按权重抽取） |

### Mod

`client-injector/scripts/baseline.ps1` 生成 `client-injector/baseline.json`（两服版本、架构与关键文件 SHA-256）。示例 Mod：

- `client-injector/Mods/example.mod` — 普通明文 Lua 进入客户端运行时
- `client-injector/Mods/future-chapter.mod` — JP 1.4.0 主线新增「未来編」大章节
- `client-injector/Mods/custom-equipment.mod` — 克隆现有装备资源，加入试验装备

xLua Mod Loader 为实验功能，只接受基线中已验证的 `xlua.dll` SHA-256；未知版本会记录 `hook refused` 并保持客户端不变。调试日志见 `client-injector/native/bin-x86/BlueOath.Payload.log`。

## 项目结构

```
.
├── src/
│   ├── BlueOath.Protocol/        # wire 协议：protobuf 信封 / KCP 编解码 / 玩家数据
│   ├── BlueOath.Core/            # Mod/客户端领域实体
│   ├── BlueOath.Tools/           # IL2CPP/协议/配置分析工具
│   ├── BlueOath.Launcher/        # 命令行客户端启动器
│   ├── BlueOath.Mods/            # Mod 清单/依赖/加载顺序发现
│   └── BlueOath.Bootstrap/       # 引导
├── rust-server/                  # 唯一服务端：Rust workspace、catalog、运行配置
├── archive/                      # 已移除的 C# 服务端栈，仅作历史恢复
├── client-injector/              # x86 注入、Payload、Injector、Mods 与专属脚本
├── lua_tools/                    # 国服/日服反编译 Lua 源码（交叉验证）
├── runtime/                      # 本地运行时数据（存档、TLS 材料）
├── tools/                        # 逆向分析与配置辅助脚本
├── docs/                         # 文档中心（见下文）
├── blueoath/  苍蓝誓约/           # 原始日服 / 国服客户端（保持不变）
└── BlueOath.Local.sln
```

## 当前进度

| 系统 | 状态 |
| --- | --- |
| SDK 登录 / 服务器列表 / 选服 | ✅ |
| 游戏登录（TCP + KCP）与主界面 HomePage | ✅ |
| 3D 看板船娘加载 | ✅ |
| 船坞 / 船娘详情 / 图鉴 | ✅ |
| 商店（GM 免费购买）/ 邮件 / 装备 / 抽卡 / 个人资料 / 舰娘升级 | ✅ |
| 编队 / 战斗进入 / 伤害结算（主炮·鱼雷·空袭·副炮） | ✅ |
| 海域索敌（迷雾 / 巡逻 / 决斗） | 🔄 持续修复中 |
| 公会 / 好友 / 聊天 / 活动等多人与活动系统 | ⬜ 离线占位响应 |

## 文档

全部文档索引见 [docs/README.md](docs/README.md)，主要入口：

| 分类 | 文档 |
| --- | --- |
| 总体规划 | [项目概述](docs/project-overview.md) · [Roadmap](docs/roadmap.zh-CN.md) · [历史复盘](archive/dotnet-scripts/retrospective.md) |
| 开发与发布 | [development/](docs/development/README.md)（代码规范 · 协议覆盖 · 启动器发布/自动更新） |
| 逆向研究 | [battle/](docs/research/battle/battle-system.md)（战斗系统 · 攻击 MISS · 自律） · [sea/](docs/research/sea/sea-battle.md)（海域玩法） · [transport](docs/research/transport.md) |
| 生成知识库 | [protocol-catalog](docs/protocol-catalog/README.zh-CN.md) · [config-catalog](docs/config-catalog/README.zh-CN.md) · [il2cpp-catalog](docs/il2cpp-catalog/README.zh-CN.md) · [lua-catalog](docs/lua-catalog/README.md) |

## Roadmap 与更新日志

- 分阶段目标与完成门槛：**[docs/roadmap.zh-CN.md](docs/roadmap.zh-CN.md)**
- 版本更新记录：**[CHANGELOG.md](CHANGELOG.md)**

## 社区

加入QQ群：
![QQ群](imgs/qrcode_1787883880894.jpg)

加入QQ频道：
![QQ频道](imgs/qrcode_1787883856963.jpg)


## 免责声明

本项目**仅用于个人学习与研究目的**。游戏及其原始资产（客户端、配置、美术与音频等）版权归原作者所有；本仓库不包含官方服务器代码。请勿将本项目用于任何商业用途。
