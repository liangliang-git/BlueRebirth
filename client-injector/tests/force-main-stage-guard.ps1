$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$hookFiles = @(
    (Join-Path $repoRoot 'native/Payload/hooks_debug.cpp'),
    (Join-Path $repoRoot 'native/Payload/hooks_product.cpp')
)

foreach ($hookFile in $hookFiles) {
    $source = Get-Content -Raw -LiteralPath $hookFile
    $compact = $source -replace '\s+', ' '

    if ($compact -match 'if \(getUserExtraSeen .*? ForceMainStage\(\)') {
        throw "ForceMainStage is still called from login worker loop: $hookFile"
    }

    if ($source -notmatch 'ForceMainStage disabled') {
        throw "Missing explicit ForceMainStage disable marker: $hookFile"
    }
}

Write-Output 'PASS: login worker cannot force Unity StageMgr.Goto during guide flow.'
