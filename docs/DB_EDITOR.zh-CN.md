# 日服客户端 DB 文件编辑器

项目新增 `BlueOath.DbEditor.App` Windows Forms 工具，直接编辑日服客户端：

`C:\Users\zhanl\Desktop\BlueOath Rebirth\blueoath\blueoath_Data\StreamingAssets\config`

## 启动

在项目根目录执行：

```powershell
dotnet run --project src\BlueOath.DbEditor.App\BlueOath.DbEditor.App.csproj -c Release
```

也可以把配置目录作为第一个参数传入：

```powershell
dotnet run --project src\BlueOath.DbEditor.App\BlueOath.DbEditor.App.csproj -c Release -- "D:\blueoath\blueoath_Data\StreamingAssets\config"
```

## 功能

- 自动扫描 `config_*.db`，只显示含 `DBObject` 表的文件。
- 按表名、ID、IndexID、JSON 内容搜索。
- XOR `0x55` 解码 `DBObject.jsonbytes`，编辑后重新编码保存。
- JSON 格式化、已有行保存、新增行、删除行。
- 每次写入前自动创建带时间戳的 `.bak` 文件。
- `nill` 元数据行和非法 JSON 行只读，避免误损坏客户端配置。

关闭客户端后再编辑配置。保存前建议复制整个 `config` 目录；游戏更新可能覆盖修改。
