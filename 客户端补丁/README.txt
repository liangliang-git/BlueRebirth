蓝色誓约本地服务端 + 日服客户端补丁

使用方法
1. 将“客户端补丁”整个文件夹放进游戏根目录。
2. 确认目录结构如下：

   游戏根目录\blueoath\blueoath.exe
   游戏根目录\客户端补丁\start-rust-game.bat

3. 双击 BlueOath.Local.exe。
4. 如需使用旧批处理入口，仍可双击 start-rust-game.bat。
5. 关闭游戏后，服务端仍可能继续运行；再次启动前先关闭旧服务端，或结束占用 7080/7201 端口的进程。

已包含
- server\blueoath-server.exe：Rust Release 服务端
- server\catalog：日服配置、掉落配置、商店/邮件配置、账号存档
- client-patch\native：x86 注入器和 Payload
- client-patch\Mods：Lua 客户端补丁
- start-rust-client-only.bat：服务端已运行时只启动客户端
- BlueOath.Local.exe：读取 launcher.json 的一键启动器
- `BlueOath.Local.exe --check-config`：只校验 JSON，不启动服务端和客户端

配置
- launcher.json：启动器唯一配置入口。
- serverIp：客户端重定向目标 IP 或域名。
- httpPort：服务端 HTTP 端口。
- gameLoginPort：服务端游戏登录 TCP 端口。
- kcpGameLoginPort：服务端游戏登录 KCP 端口。
- redirectPort：Payload 重定向端口。
- logLevel：`off`、`error`、`warn`、`info`、`debug`、`trace`。
- startLocalServer=false：不启动包内服务端，连接配置中的远程服务端。

日志
- server\rust-server.log
- client-patch\native\bin-x86\BlueOath.Payload.log

说明
- 仅支持当前日服 Windows 客户端，GameAssembly.dll 哈希已固定。
- 补丁包不包含原始客户端资源；原客户端必须已经在游戏根目录的 blueoath 文件夹中。
- 账号数据保存于 server\catalog\data\profiles.db。
- 服务端端口：HTTP 7080；游戏登录 TCP/KCP 7201；客户端重定向端口 10173。
- 若客户端更新导致 GameAssembly.dll 哈希变化，需要重新生成并替换补丁包中的 GAME_HASH 和 native 文件。
