$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
$modMain = Join-Path $root 'Mods\disable-recharge-purchase.mod\main.lua'
$modManifest = Join-Path $root 'Mods\disable-recharge-purchase.mod\mod.json'
$bootstrap = Join-Path $root 'Mods\bootstrap.lua'

if (-not (Test-Path -LiteralPath $modMain)) { throw 'purchase-disable mod entry is missing' }
if (-not (Test-Path -LiteralPath $modManifest)) { throw 'purchase-disable mod manifest is missing' }

$mainText = Get-Content -LiteralPath $modMain -Raw
$manifestText = Get-Content -LiteralPath $modManifest -Raw
$bootstrapText = Get-Content -LiteralPath $bootstrap -Raw

if ($mainText -notmatch 'BuyRechargeItem|_BuyItem|_BuyItemImp') {
    throw 'purchase-disable mod does not intercept recharge purchase callbacks'
}
if ($mainText -notmatch 'btn_buy') {
    throw 'purchase-disable mod does not hide purchase button'
}
if ($manifestText -notmatch '"enabled"\s*:\s*true') {
    throw 'purchase-disable mod is not enabled'
}
if ($bootstrapText -notmatch 'disable-recharge-purchase\.mod/main\.lua') {
    throw 'purchase-disable mod is not registered in bootstrap'
}

Write-Output 'disable-recharge-purchase static checks passed'
