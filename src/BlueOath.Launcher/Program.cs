using System.Diagnostics;
using System.Net.Sockets;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace BlueOath.Launcher;

internal static class Program
{
    internal const string DefaultGameHash = "8AEE607813A759E047D81C2428990609322DE072437DD4597F80E8E3FAD1D404";

    public static async Task<int> Main(string[] args)
    {
        var baseDirectory = AppContext.BaseDirectory;
        var configPath = GetOption(args, "--config") ?? Path.Combine(baseDirectory, "launcher.json");

        try
        {
            var config = LauncherConfig.Load(configPath);
            var logger = new LauncherLogger(ResolvePath(baseDirectory, config.LogFile), LogLevels.Parse(config.LogLevel));
            logger.Info($"launcher config={configPath}");
            if (HasFlag(args, "--check-config"))
            {
                logger.Info("launcher config valid");
                return 0;
            }

            var clientRoot = ResolvePath(baseDirectory, config.ClientRoot);
            var nativeRoot = ResolvePath(baseDirectory, config.NativeRoot);
            var clientExecutable = Path.Combine(clientRoot, config.Region.Equals("cn", StringComparison.OrdinalIgnoreCase)
                ? "clsy.exe"
                : "blueoath.exe");
            var gameAssembly = Path.Combine(clientRoot, "GameAssembly.dll");
            EnsureFile(clientExecutable, "客户端主程序");
            EnsureFile(gameAssembly, "GameAssembly.dll");

            if (HasFlag(args, "--original"))
            {
                logger.Info("starting original client");
                var original = Process.Start(new ProcessStartInfo(clientExecutable)
                {
                    WorkingDirectory = clientRoot,
                    UseShellExecute = true,
                });
                return original is null ? 3 : 0;
            }

            var injector = Path.Combine(nativeRoot, "bin-x86", "BlueOath.Injector.exe");
            var injectScript = Path.Combine(baseDirectory, "client-patch", "scripts", "inject-game.ps1");
            EnsureFile(injector, "客户端注入器");
            EnsureFile(injectScript, "注入脚本");

            Process? server = null;
            if (config.StartLocalServer && !HasFlag(args, "--no-server"))
            {
                server = await StartOrReuseServerAsync(baseDirectory, config, logger);
                var ready = await WaitForPortAsync(config.ServerIp, config.HttpPort, config.StartupTimeoutSeconds);
                if (!ready)
                {
                    logger.Error($"server did not open {config.ServerIp}:{config.HttpPort}");
                    TryStop(server);
                    return 4;
                }
                logger.Info($"server ready {config.ServerIp}:{config.HttpPort}");
            }

            var exitCode = await StartInjectedClientAsync(config, clientRoot, nativeRoot, injectScript, logger);
            if (config.StopServerOnExit) TryStop(server);
            return exitCode;
        }
        catch (Exception error)
        {
            Console.Error.WriteLine(error.Message);
            return 2;
        }
    }

    private static async Task<Process> StartOrReuseServerAsync(
        string baseDirectory,
        LauncherConfig config,
        LauncherLogger logger)
    {
        var serverExecutable = ResolvePath(baseDirectory, config.ServerExecutable);
        EnsureFile(serverExecutable, "Rust 服务端");

        var existing = FindProcessByExecutable(serverExecutable);
        if (existing is not null)
        {
            if (await WaitForPortAsync(config.ServerIp, config.HttpPort, 2))
            {
                logger.Info($"reusing server pid={existing.Id}");
                return existing;
            }

            logger.Write(LogLevel.Warn, $"stopping stale server pid={existing.Id}");
            TryStop(existing);
            try { await existing.WaitForExitAsync().WaitAsync(TimeSpan.FromSeconds(3)); } catch (Exception) { }
        }

        return StartServer(baseDirectory, config, logger);
    }

    private static Process StartServer(string baseDirectory, LauncherConfig config, LauncherLogger logger)
    {
        var serverExecutable = ResolvePath(baseDirectory, config.ServerExecutable);
        var serverConfig = ResolvePath(baseDirectory, config.ServerConfig);
        var serverData = ResolvePath(baseDirectory, config.ServerData);
        EnsureFile(serverExecutable, "Rust 服务端");
        EnsureFile(serverConfig, "服务端配置");
        if (!Directory.Exists(serverData)) throw new DirectoryNotFoundException($"服务端数据目录不存在: {serverData}");

        var startInfo = new ProcessStartInfo(serverExecutable)
        {
            WorkingDirectory = Path.GetDirectoryName(serverExecutable) ?? baseDirectory,
            UseShellExecute = false,
            CreateNoWindow = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
        };
        AddArgument(startInfo, "--config", serverConfig);
        AddArgument(startInfo, "--port", config.HttpPort.ToString());
        AddArgument(startInfo, "--game-login-port", config.GameLoginPort.ToString());
        AddArgument(startInfo, "--kcp-game-login-port", config.KcpGameLoginPort.ToString());
        AddArgument(startInfo, "--client-path", ResolvePath(baseDirectory, config.ClientRoot));
        AddArgument(startInfo, "--data", serverData);
        AddArgument(startInfo, "--profile-id", config.ProfileId);

        var process = Process.Start(startInfo) ?? throw new InvalidOperationException("无法启动 Rust 服务端");
        _ = PumpAsync(process.StandardOutput, logger, LogLevel.Info, "server");
        _ = PumpAsync(process.StandardError, logger, LogLevel.Error, "server");
        logger.Info($"server started pid={process.Id}");
        return process;
    }

    private static async Task<int> StartInjectedClientAsync(
        LauncherConfig config,
        string clientRoot,
        string nativeRoot,
        string injectScript,
        LauncherLogger logger)
    {
        var powershell = OperatingSystem.IsWindows() ? "powershell.exe" : "pwsh";
        var startInfo = new ProcessStartInfo(powershell)
        {
            WorkingDirectory = clientRoot,
            UseShellExecute = false,
            CreateNoWindow = false,
        };
        startInfo.ArgumentList.Add("-NoProfile");
        startInfo.ArgumentList.Add("-ExecutionPolicy");
        startInfo.ArgumentList.Add("Bypass");
        startInfo.ArgumentList.Add("-File");
        startInfo.ArgumentList.Add(injectScript);
        AddArgument(startInfo, "-Region", config.Region);
        AddArgument(startInfo, "-ClientRoot", clientRoot);
        AddArgument(startInfo, "-NativeRoot", nativeRoot);
        AddArgument(startInfo, "-GameHash", config.GameHash);
        AddArgument(startInfo, "-ServerHost", config.ServerIp);
        AddArgument(startInfo, "-Port", config.RedirectPort.ToString());
        AddArgument(startInfo, "-HttpPort", config.HttpPort.ToString());
        AddArgument(startInfo, "-LogLevel", config.LogLevel);
        if (config.Redirect) startInfo.ArgumentList.Add("-Redirect");
        if (config.AllowUntrusted) startInfo.ArgumentList.Add("-AllowUntrusted");
        if (config.BypassSdk) startInfo.ArgumentList.Add("-BypassSdk");

        logger.Info($"starting client region={config.Region} target={config.ServerIp}:{config.HttpPort} logLevel={config.LogLevel}");
        using var process = Process.Start(startInfo) ?? throw new InvalidOperationException("无法启动客户端注入脚本");
        await process.WaitForExitAsync();
        logger.Info($"client stopped exitCode={process.ExitCode}");
        return process.ExitCode;
    }

    private static async Task PumpAsync(StreamReader reader, LauncherLogger logger, LogLevel level, string source)
    {
        while (await reader.ReadLineAsync() is { } line) logger.Write(level, $"[{source}] {line}");
    }

    private static async Task<bool> WaitForPortAsync(string host, int port, int timeoutSeconds)
    {
        var deadline = DateTime.UtcNow.AddSeconds(timeoutSeconds);
        while (DateTime.UtcNow < deadline)
        {
            try
            {
                using var client = new TcpClient();
                await client.ConnectAsync(host, port).WaitAsync(TimeSpan.FromMilliseconds(500));
                return true;
            }
            catch (Exception) when (DateTime.UtcNow < deadline)
            {
                await Task.Delay(250);
            }
        }
        return false;
    }

    private static void AddArgument(ProcessStartInfo startInfo, string name, string value)
    {
        startInfo.ArgumentList.Add(name);
        startInfo.ArgumentList.Add(value);
    }

    private static string ResolvePath(string baseDirectory, string path) =>
        Path.GetFullPath(Path.IsPathRooted(path) ? path : Path.Combine(baseDirectory, path));

    private static Process? FindProcessByExecutable(string executablePath)
    {
        var expectedPath = Path.GetFullPath(executablePath);
        var processName = Path.GetFileNameWithoutExtension(expectedPath);
        foreach (var process in Process.GetProcessesByName(processName))
        {
            try
            {
                var actualPath = process.MainModule?.FileName;
                if (!process.HasExited && actualPath is not null &&
                    string.Equals(Path.GetFullPath(actualPath), expectedPath, StringComparison.OrdinalIgnoreCase))
                    return process;
            }
            catch (Exception)
            {
                // Access to another process can fail; do not treat it as our server.
            }

            process.Dispose();
        }

        return null;
    }

    private static void EnsureFile(string path, string description)
    {
        if (!File.Exists(path)) throw new FileNotFoundException($"{description}不存在: {path}", path);
    }

    private static void TryStop(Process? process)
    {
        if (process is null || process.HasExited) return;
        try { process.Kill(entireProcessTree: true); } catch (InvalidOperationException) { }
    }

    private static bool HasFlag(string[] args, string value) =>
        args.Any(arg => arg.Equals(value, StringComparison.OrdinalIgnoreCase));

    private static string? GetOption(string[] args, string name)
    {
        var prefix = name + "=";
        return args.FirstOrDefault(arg => arg.StartsWith(prefix, StringComparison.OrdinalIgnoreCase))?[prefix.Length..];
    }
}

internal sealed class LauncherConfig
{
    [JsonPropertyName("region")] public string Region { get; set; } = "jp";
    [JsonPropertyName("serverIp")] public string ServerIp { get; set; } = "127.0.0.1";
    [JsonPropertyName("httpPort")] public int HttpPort { get; set; } = 7080;
    [JsonPropertyName("gameLoginPort")] public int GameLoginPort { get; set; } = 7201;
    [JsonPropertyName("kcpGameLoginPort")] public int KcpGameLoginPort { get; set; } = 7201;
    [JsonPropertyName("redirectPort")] public int RedirectPort { get; set; } = 10173;
    [JsonPropertyName("startLocalServer")] public bool StartLocalServer { get; set; } = true;
    [JsonPropertyName("stopServerOnExit")] public bool StopServerOnExit { get; set; }
    [JsonPropertyName("startupTimeoutSeconds")] public int StartupTimeoutSeconds { get; set; } = 20;
    [JsonPropertyName("profileId")] public string ProfileId { get; set; } = "local-player";
    [JsonPropertyName("clientRoot")] public string ClientRoot { get; set; } = "..\\blueoath";
    [JsonPropertyName("nativeRoot")] public string NativeRoot { get; set; } = "client-patch\\native";
    [JsonPropertyName("serverExecutable")] public string ServerExecutable { get; set; } = "server\\blueoath-server.exe";
    [JsonPropertyName("serverConfig")] public string ServerConfig { get; set; } = "server\\server.json";
    [JsonPropertyName("serverData")] public string ServerData { get; set; } = "server\\catalog\\data";
    [JsonPropertyName("logFile")] public string LogFile { get; set; } = "launcher.log";
    [JsonPropertyName("logLevel")] public string LogLevel { get; set; } = "info";
    [JsonPropertyName("gameHash")] public string GameHash { get; set; } = Program.DefaultGameHash;
    [JsonPropertyName("redirect")] public bool Redirect { get; set; } = true;
    [JsonPropertyName("allowUntrusted")] public bool AllowUntrusted { get; set; } = true;
    [JsonPropertyName("bypassSdk")] public bool BypassSdk { get; set; }

    public static LauncherConfig Load(string path)
    {
        if (!File.Exists(path)) throw new FileNotFoundException($"启动器配置不存在: {path}", path);
        var options = new JsonSerializerOptions { PropertyNameCaseInsensitive = true };
        var config = JsonSerializer.Deserialize<LauncherConfig>(File.ReadAllText(path), options)
            ?? throw new InvalidDataException($"启动器配置为空: {path}");
        config.Validate();
        return config;
    }

    private void Validate()
    {
        if (!Region.Equals("jp", StringComparison.OrdinalIgnoreCase) && !Region.Equals("cn", StringComparison.OrdinalIgnoreCase))
            throw new InvalidDataException($"region 只支持 jp/cn，当前值: {Region}");
        if (string.IsNullOrWhiteSpace(ServerIp)) throw new InvalidDataException("serverIp 不能为空");
        ValidatePort(nameof(HttpPort), HttpPort);
        ValidatePort(nameof(GameLoginPort), GameLoginPort);
        ValidatePort(nameof(KcpGameLoginPort), KcpGameLoginPort);
        ValidatePort(nameof(RedirectPort), RedirectPort);
        if (StartupTimeoutSeconds is < 1 or > 300) throw new InvalidDataException("startupTimeoutSeconds 必须在 1 到 300 之间");
        _ = LogLevels.Parse(LogLevel);
        if (string.IsNullOrWhiteSpace(GameHash)) throw new InvalidDataException("gameHash 不能为空");
    }

    private static void ValidatePort(string name, int value)
    {
        if (value is < 1 or > 65535) throw new InvalidDataException($"{name} 必须在 1 到 65535 之间");
    }
}

internal enum LogLevel
{
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
    Trace = 4,
    Off = 5,
}

internal static class LogLevels
{
    public static LogLevel Parse(string value) => value.Trim().ToLowerInvariant() switch
    {
        "off" => LogLevel.Off,
        "error" => LogLevel.Error,
        "warn" or "warning" => LogLevel.Warn,
        "info" => LogLevel.Info,
        "debug" => LogLevel.Debug,
        "trace" => LogLevel.Trace,
        _ => throw new InvalidDataException($"logLevel 不支持: {value}"),
    };
}

internal sealed class LauncherLogger
{
    private readonly string _path;
    private readonly LogLevel _minimum;
    private readonly object _sync = new();

    public LauncherLogger(string path, LogLevel minimum)
    {
        _path = path;
        _minimum = minimum;
        var directory = Path.GetDirectoryName(path);
        if (!string.IsNullOrEmpty(directory)) Directory.CreateDirectory(directory);
    }

    public void Info(string message) => Write(LogLevel.Info, message);
    public void Error(string message) => Write(LogLevel.Error, message);

    public void Write(LogLevel level, string message)
    {
        if (_minimum == LogLevel.Off && level != LogLevel.Error) return;
        if (_minimum != LogLevel.Off && level > _minimum) return;
        var line = $"[{DateTime.Now:yyyy-MM-dd HH:mm:ss.fff}] [{level.ToString().ToUpperInvariant()}] {message}";
        lock (_sync)
        {
            Console.WriteLine(line);
            File.AppendAllText(_path, line + Environment.NewLine);
        }
    }
}
