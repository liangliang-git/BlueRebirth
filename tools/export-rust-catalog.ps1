param(
  [string]$ClientRoot = 'E:\BlueOath Rebirth\blueoath',
  [string]$OutputRoot = '',
  [string]$DataRoot = '',
  [ValidateSet('json', 'db', 'both')]
  [string]$ConfigFormat = 'json'
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$source = Join-Path (Resolve-Path -LiteralPath $ClientRoot).Path 'blueoath_Data\StreamingAssets\config'
$destination = if ($OutputRoot) {
  if ([System.IO.Path]::IsPathRooted($OutputRoot)) {
    [System.IO.Path]::GetFullPath($OutputRoot)
  } else {
    [System.IO.Path]::GetFullPath((Join-Path (Get-Location).Path $OutputRoot))
  }
} else {
  Join-Path $repoRoot 'rust-server\catalog\server-config'
}
New-Item -ItemType Directory -Path $destination -Force | Out-Null

$files = Get-ChildItem -LiteralPath $source -File -Filter 'config_*.db' | Sort-Object Name
if ($files.Count -eq 0) {
  throw "No config_*.db files found under $source"
}

if ($ConfigFormat -in @('db', 'both')) {
  foreach ($file in $files) {
    Copy-Item -LiteralPath $file.FullName -Destination (Join-Path $destination $file.Name) -Force
  }
}

if ($ConfigFormat -in @('json', 'both')) {
  $jsonExporter = Join-Path $repoRoot 'tools\export-rust-catalog-json.py'
  $python = Get-Command python -ErrorAction SilentlyContinue
  if (-not $python) {
    $python = Get-Command py -ErrorAction SilentlyContinue
  }
  if (-not $python) {
    throw 'Python 3 is required for JSON catalog export'
  }
  & $python.Source $jsonExporter --source $source --output $destination
  if ($LASTEXITCODE -ne 0) {
    throw "JSON catalog export failed with exit code $LASTEXITCODE"
  }
  $jsonPruner = Join-Path $repoRoot 'tools\prune-rust-catalog-json.py'
  & $python.Source $jsonPruner --apply $destination
  if ($LASTEXITCODE -ne 0) {
    throw "JSON catalog field pruning failed with exit code $LASTEXITCODE"
  }
}

$requiredShopFiles = if ($ConfigFormat -eq 'db') {
  @('config_shop.db', 'config_shop_goods.db')
} elseif ($ConfigFormat -eq 'json') {
  @('config_shop.json', 'config_shop_goods.json')
} else {
  @('config_shop.db', 'config_shop_goods.db', 'config_shop.json', 'config_shop_goods.json')
}
foreach ($fileName in $requiredShopFiles) {
  if (-not (Test-Path -LiteralPath (Join-Path $destination $fileName))) {
    throw "Required shop catalog is missing: $fileName"
  }
}

$runtimeDestination = if ($DataRoot) {
  if ([System.IO.Path]::IsPathRooted($DataRoot)) {
    [System.IO.Path]::GetFullPath($DataRoot)
  } else {
    [System.IO.Path]::GetFullPath((Join-Path (Get-Location).Path $DataRoot))
  }
} else {
  Join-Path (Split-Path -Parent $destination) 'data'
}
New-Item -ItemType Directory -Path $runtimeDestination -Force | Out-Null
$runtimeSource = Join-Path $repoRoot 'rust-server\catalog\data'
foreach ($fileName in @('announcements.json', 'gm-goods.json', 'gm-mails.json', 'build-pools.json')) {
  $runtimePath = Join-Path $runtimeSource $fileName
  if (-not (Test-Path -LiteralPath $runtimePath -PathType Leaf)) {
    throw "Server runtime data file is missing: $runtimePath"
  }
  $runtimeDestinationPath = Join-Path $runtimeDestination $fileName
  if ([System.IO.Path]::GetFullPath($runtimePath) -ne [System.IO.Path]::GetFullPath($runtimeDestinationPath)) {
    Copy-Item -LiteralPath $runtimePath -Destination $runtimeDestinationPath -Force
  }
}

$bytes = ($files | Measure-Object -Property Length -Sum).Sum
Write-Host "Exported $($files.Count) config databases ($bytes source bytes) as $ConfigFormat to $destination" -ForegroundColor Green
Write-Host "Copied 4 server runtime JSON files to $runtimeDestination" -ForegroundColor Green

if ($ConfigFormat -in @('json', 'both')) {
  $catalogRoot = Split-Path -Parent $destination
  $catalogDbBuilder = Join-Path $repoRoot 'tools\build-catalog-db.py'
  $catalogDb = Join-Path (Split-Path -Parent $catalogRoot) 'server_config.db'
  & $python.Source $catalogDbBuilder --catalog-root $catalogRoot --output $catalogDb
  if ($LASTEXITCODE -ne 0) {
    throw "catalog database build failed with exit code $LASTEXITCODE"
  }
  Write-Host "Built server_config.db under $(Split-Path -Parent $catalogRoot)" -ForegroundColor Green
}
