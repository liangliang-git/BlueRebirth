param(
  [ValidateSet('jp','cn')][string]$Region = 'jp',
  [switch]$Redirect,
  [ValidateRange(0,65535)][int]$Port = 0,
  [ValidateRange(0,65535)][int]$HttpPort = 0,
  [string]$ServerHost = '127.0.0.1',
  [ValidateSet('off','error','warn','info','debug','trace')][string]$LogLevel = 'info',
  [string]$ClientRoot = '',
  [string]$NativeRoot = '',
  [string]$GameHash = '',
  [string]$TrustCertificate = '',
  [switch]$AllowUntrusted,
  [switch]$BypassSdk,
  [string]$GameArguments = ''
)
$ErrorActionPreference = 'Stop'
$clientBundleRoot = Split-Path -Parent $PSScriptRoot
$root = Split-Path -Parent $clientBundleRoot
$baseline = Get-Content -LiteralPath (Join-Path $clientBundleRoot 'baseline.json') -Raw | ConvertFrom-Json
$client = if ($ClientRoot) {
  (Resolve-Path -LiteralPath $ClientRoot).Path
} elseif ($Region -eq 'jp') {
  Join-Path $root 'blueoath\blueoath'
} else {
  (Get-ChildItem -LiteralPath $root -Directory | ForEach-Object { Get-ChildItem -LiteralPath $_.FullName -Directory -Filter clsy -ErrorAction SilentlyContinue } | Select-Object -First 1).FullName
}
$exe = Join-Path $client $(if ($Region -eq 'jp') { 'blueoath.exe' } else { 'clsy.exe' })
$nativeBase = if ($NativeRoot) { (Resolve-Path -LiteralPath $NativeRoot).Path } else { Join-Path $clientBundleRoot 'native' }
$native = if (Test-Path -LiteralPath (Join-Path $nativeBase 'BlueOath.Injector.exe')) {
  $nativeBase
} elseif (Test-Path -LiteralPath (Join-Path $nativeBase 'bin-x86')) {
  Join-Path $nativeBase 'bin-x86'
} else {
  Join-Path $nativeBase 'bin-x86'
}
$injector = Join-Path $native 'BlueOath.Injector.exe'
$payload = Join-Path $native 'BlueOath.Payload.dll'
if (-not (Test-Path -LiteralPath $injector)) {
  if ($NativeRoot) { throw "Injector missing: $injector" }
  & (Join-Path $PSScriptRoot 'build-native.ps1')
}
$nativeConfig = Join-Path $native 'bootstrap.ini'
$nativeRootConfig = Join-Path $nativeBase 'bootstrap.ini'
$repoConfig = Join-Path $clientBundleRoot 'native\bootstrap.ini'
$configTemplate = if ($NativeRoot -and (Test-Path -LiteralPath $nativeRootConfig)) {
  $nativeRootConfig
} elseif (Test-Path -LiteralPath $nativeConfig) {
  $nativeConfig
} else {
  $repoConfig
}
if (-not (Test-Path -LiteralPath $configTemplate)) { throw "bootstrap.ini missing: $configTemplate" }
if ($configTemplate -ne $nativeConfig) { Copy-Item -LiteralPath $configTemplate -Destination $nativeConfig -Force }
$config = Join-Path $native 'bootstrap.ini'
$enabled = if ($Redirect) { 1 } else { 0 }
$configLines = @('[redirect]',"enabled=$enabled","host=$ServerHost","port=$Port","http_port=$HttpPort")
$srcConfig = $configTemplate
if (Test-Path -LiteralPath $srcConfig) {
  $srcLines = @(Get-Content -LiteralPath $srcConfig -Encoding Unicode)
  $captureBugly = ($srcLines | Where-Object { $_ -match '^\s*capture_bugly\s*=\s*(\d+)' } | ForEach-Object { $Matches[1] } | Select-Object -First 1)
  $capturePort = ($srcLines | Where-Object { $_ -match '^\s*capture_port\s*=\s*(\d+)' } | ForEach-Object { $Matches[1] } | Select-Object -First 1)
  if ($captureBugly) { $configLines += "capture_bugly=$captureBugly" }
  if ($capturePort) { $configLines += "capture_port=$capturePort" }

  # Keep extension sections (for example [mods]) verbatim. Payload builds may add
  # settings unknown to this script; dropping them silently disables those modules.
  $knownSections = @('redirect', 'trust', 'sdk', 'debug')
  $unknownLines = @()
  $section = ''
  foreach ($line in $srcLines) {
    if ($line -match '^\s*\[([^\]]+)\]') { $section = $Matches[1].Trim().ToLowerInvariant() }
    if ($section -and ($knownSections -notcontains $section)) { $unknownLines += $line }
  }
  if ($unknownLines.Count -gt 0) { $configLines += $unknownLines }
}
if ($TrustCertificate) {
  $resolvedTrust = (Resolve-Path -LiteralPath $TrustCertificate).Path
  $configLines += @('[trust]',"certificate=$resolvedTrust")
}
if ($AllowUntrusted) {
  if (-not $TrustCertificate) { $configLines += '[trust]' }
  $configLines += 'allow_untrusted=1'
}
if ($BypassSdk) {
  $configLines += @('[sdk]', 'bypass=1')
}
$diagnostics = if ($LogLevel -in @('debug', 'trace')) { 1 } else { 0 }
$configLines += @('[debug]', "log_level=$LogLevel", "diagnostics=$diagnostics")
Set-Content -LiteralPath $config -Encoding Unicode -Value $configLines
$entry = $baseline | Where-Object region -eq $Region
$resolvedGameHash = if ($GameHash) { $GameHash } else { ($entry.files.PSObject.Properties | Where-Object Name -like '*GameAssembly.dll').Value }
$injectorArgs = @("--exe=$exe", "--payload=$payload", "--game-hash=$resolvedGameHash")
if ($GameArguments) { $injectorArgs += "--args=$GameArguments" }
$injectorExitCode = 1
Push-Location -LiteralPath $native
try {
  # Match direct injector launches: client-injector/native/bin-x86 is the
  # process working directory used by payload-relative lookups.
  & $injector @injectorArgs
  $injectorExitCode = $LASTEXITCODE
} finally {
  Pop-Location
}
exit $injectorExitCode
