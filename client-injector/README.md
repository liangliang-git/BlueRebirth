# Client injector

Client-only runtime components. This directory is independent from the Rust
server.

- `native/`: x86 Injector, Payload, Lua loader, and native build config.
- `Mods/`: client Lua Mod source and `bootstrap.lua`.
- `scripts/`: native build, baseline generation, client injection, and TLS proxy.
- `baseline.json`: verified client file hashes used by injection.

Build native components:

```powershell
powershell -File .\client-injector\scripts\build-native.ps1
```

Start Rust server and client injection from repository root:

```powershell
.\run-game.bat
```
