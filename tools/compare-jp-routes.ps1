[CmdletBinding()]
param(
    [string]$ClientRoot = 'C:\Users\zhanl\Desktop\BlueOath Rebirth',
    [string]$NormalizedServiceRoot = '',
    [string]$ProbePath = '',
    [string]$ServerSourceRoot = '',
    [string]$ProtobufTypeManagerPath = ''
)

$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($NormalizedServiceRoot)) {
    $NormalizedServiceRoot = Join-Path $ClientRoot 'runtime\lua-normalized\service'
}
if ([string]::IsNullOrWhiteSpace($ProbePath)) {
    $ProbePath = Join-Path $PSScriptRoot 'lua-probe\target\debug\lua-probe.exe'
}
if ([string]::IsNullOrWhiteSpace($ServerSourceRoot)) {
    $ServerSourceRoot = Join-Path $PSScriptRoot '..\rust-server\crates\server\src'
}
if ([string]::IsNullOrWhiteSpace($ProtobufTypeManagerPath)) {
    $ProtobufTypeManagerPath = Join-Path $ClientRoot 'runtime\lua-normalized\net\protobuftypemanager.lua'
}

if (-not (Test-Path -LiteralPath $NormalizedServiceRoot)) {
    throw "normalized service directory not found: $NormalizedServiceRoot"
}
if (-not (Test-Path -LiteralPath $ProbePath)) {
    throw "lua probe not found: $ProbePath"
}
if (-not (Test-Path -LiteralPath $ServerSourceRoot)) {
    throw "server source directory not found: $ServerSourceRoot"
}

$routes = [System.Collections.Generic.HashSet[string]]::new()
Get-ChildItem -LiteralPath $NormalizedServiceRoot -Filter '*.lua' -File -Recurse | ForEach-Object {
    $decompiled = (& $ProbePath $_.FullName 2>$null | Out-String)
    [regex]::Matches($decompiled, 'SendNetEvent\("([^"]+)"') | ForEach-Object {
        [void]$routes.Add($_.Groups[1].Value)
    }
}

# Some JP Lua services call SendNetEvent through a local function variable, so
# the route string is not adjacent to the method name in decompiled output.
# Keep verified dynamic-call literals here and review when client version changes.
@(
    'bag.SaleBagItem',
    'battle.SendAutoMsg',
    'dailycopy.GetData',
    'guild.Quit',
    'illustrate.VowDecTime',
    'sportsmeetrank.GetAttackBeeRank',
    'sportsmeetrank.GetTrackRank',
    'sportsmeetrank.GetSteeplechaseRank'
) | ForEach-Object { [void]$routes.Add($_) }

# These entries mirror game_login.rs dispatcher boundaries. Keep exact routes
# here when route has no namespace; use prefixes for handler modules.
$prefixes = @(
    'hero.', 'user.', 'usersvr.', 'strategy.', 'supportfleet.', 'presetfleet.',
    'bag.', 'battle.', 'illustrate.',
    'milestone.', 'supply.', 'jopen.', 'guide.', 'guild.', 'friend.', 'shop.',
    'recharge.', 'equip.', 'equiptestcopy.', 'equipnewtestcopy.', 'equipactivity.',
    'building.', 'build.', 'buildnotes.', 'discuss.', 'buildship.', 'study.',
    'task.', 'bathroom.', 'matchsvr.', 'matchsvr_', 'room.', 'copy.', 'mopUp.',
    'dailycopy.', 'talentTree.', 'tower.', 'activityTower.', 'teachingsvr.',
    'outpost.', 'activitybirthday.', 'activitychristmasshop.',
    'activitycodeexchange.', 'activityextract.', 'activityextractur.',
    'activityfashion.', 'activitypapercut.', 'activitysecretcopy.', 'activitySSR.',
    'activitySSRrolls.', 'activityvalentineloveletter.', 'activityVideo.',
    'adventure.', 'bigactivity.', 'boss.', 'chat.', 'guildbigactivity.',
    'guildbigactivityrank.', 'guildbox.', 'guildOffer.', 'guildofferrank.',
    'guildtask.', 'guildwar.', 'heroawaken.', 'invitescore.', 'shiptask.',
    'sportsmeet.', 'sportsmeetrank.', 'worldevent.', 'worldeventrank.',
    'exchange.', 'foodCompose.', 'battlepass.', 'activitybattlepass.',
    'magazine.', 'interactionitem.'
)
$exact = @(
    'GetSvrTime', 'player.Login', 'player.GetUserInfo', 'player.GetUserList',
    'player.CreateUser', 'cachedata.CacheData', 'illustrate.IllustrateNew',
    'illustrate.EquipNew', 'illustrate.IllustrateInfo', 'illustrate.VowHero',
    'illustrate.AddBehaviour', 'illustrate.ModiVowHeroList',
    'fashion.fashionReplaceReward', 'fashion.updateData', 'fashion.Equip',
    'bag.GetBagInfo', 'bag.CompositeItem', 'bag.GetNormalTreasureInfo',
    'bag.GetSelectTreasureInfo', 'copyinfo.DotBase', 'copyinfo.GetCopyInfo',
    'copy.ChooseSfLv', 'copy.GetCopy', 'copy.UnLockCopy', 'tactic.GetHerosTactic',
    'tactic.SetHerosTactic', 'mail.GetMailList', 'mail.OpenMail', 'mail.DeleteMail',
    'mail.DeleteAllMail', 'mail.ReceiveNewMail', 'mail.FetchItem', 'mail.FetchAllItems',
    'battle.CreateRoom', 'battle.JoinRoom', 'battle.LeaveRoom', 'battle.MatchJoin',
    'battle.MatchLeave', 'battle.CreateMutiBattle', 'battle.createBattleInfo',
    'archiveCopy.IsLoad', 'copyextra.AddCopyRewardCount', 'copyextra.UpdateCopyExtraInfo', 'prefs.SavePrefs',
    'statcount.GetStatCount', 'sign.Sign', 'miniGame.StartMiniGame',
    'alchemy.StartAlchemy', 'repair.RepairHero'
)

$unresolved = @($routes | Where-Object {
    $route = $_
    ($exact -notcontains $route) -and
        -not ($prefixes | Where-Object { $route.StartsWith($_, [System.StringComparison]::Ordinal) })
} | Sort-Object)

$serverSource = (Get-ChildItem -LiteralPath $ServerSourceRoot -Filter '*.rs' -File -Recurse |
    Get-Content -Raw) -join "`n"
$serverLiteralRoutes = [System.Collections.Generic.HashSet[string]]::new()
[regex]::Matches($serverSource, '"([A-Za-z][A-Za-z0-9_]*\.[A-Za-z][A-Za-z0-9_]*)"') | ForEach-Object {
    [void]$serverLiteralRoutes.Add($_.Groups[1].Value)
}
$literalCovered = @($routes | Where-Object { $serverLiteralRoutes.Contains($_) } | Sort-Object)
$compatFallback = @($routes | Where-Object { -not $serverLiteralRoutes.Contains($_) } | Sort-Object)

# JP client keeps response protobuf type mapping in ProtobufTypeManager. This
# is separate from request route extraction: a route can be reachable while
# still returning a wire-incompatible body.
$typedRoutes = @{}
if (Test-Path -LiteralPath $ProtobufTypeManagerPath) {
    $typeSource = (& $ProbePath $ProtobufTypeManagerPath 2>$null | Out-String)
    [regex]::Matches($typeSource, 't\["([^"]+)"\]\s*=\s*([A-Za-z0-9_\.]+)') | ForEach-Object {
        $typedRoutes[$_.Groups[1].Value] = $_.Groups[2].Value
    }
}
$typedCovered = @($routes | Where-Object { $typedRoutes.ContainsKey($_) } | Sort-Object)
$typedUnmapped = @($routes | Where-Object { -not $typedRoutes.ContainsKey($_) } | Sort-Object)

# BindEvent callback scan identifies routes whose response handler really calls
# PbToLua. This separates genuine response-type gaps from write-only callbacks.
$callbackTypes = @{}
Get-ChildItem -LiteralPath $NormalizedServiceRoot -Filter '*.lua' -File -Recurse | ForEach-Object {
    $decompiled = (& $ProbePath $_.FullName 2>$null | Out-String)
    [regex]::Matches($decompiled, 'BindEvent\("([^"]+)"\s*,\s*self\.([A-Za-z0-9_]+)') | ForEach-Object {
        $route = $_.Groups[1].Value
        $callback = $_.Groups[2].Value
        $callbackPattern = '(?ms)(?:\b[A-Za-z0-9_]+\.' + [regex]::Escape($callback) + '\s*=\s*function|\bfunction\s+[A-Za-z0-9_]+:' + [regex]::Escape($callback) + '\b).*?(?=\r?\n-- line |\r?\nreturn\s+[A-Za-z0-9_]+)'
        $callbackMatch = [regex]::Match($decompiled, $callbackPattern)
        if ($callbackMatch.Success) {
            $typeMatch = [regex]::Match(
                $callbackMatch.Value,
                'PbToLua\([^,]+,\s*([A-Za-z0-9_]+)_pb\.([A-Za-z0-9_]+)'
            )
            if ($typeMatch.Success) {
                $callbackTypes[$route] = $typeMatch.Groups[1].Value + '.' + $typeMatch.Groups[2].Value
            }
        }
    }
}
$clientPbRoutes = @($routes | Where-Object { $callbackTypes.ContainsKey($_) } | Sort-Object)
$clientPbWithoutTypeManager = @($clientPbRoutes | Where-Object { -not $typedRoutes.ContainsKey($_) })

[pscustomobject]@{
    normalizedServiceRoot = $NormalizedServiceRoot
    routeCount = $routes.Count
    supportedCount = $routes.Count - $unresolved.Count
    unresolvedCount = $unresolved.Count
    unresolved = $unresolved
    serverLiteralRouteCount = $serverLiteralRoutes.Count
    clientRoutesWithServerLiteral = $literalCovered.Count
    compatFallbackCount = $compatFallback.Count
    compatFallbackRoutes = $compatFallback
    protobufTypeManagerPath = $ProtobufTypeManagerPath
    typedRouteCount = $typedRoutes.Count
    clientRoutesWithProtobufType = $typedCovered.Count
    protobufTypeUnmappedCount = $typedUnmapped.Count
    protobufTypeUnmappedRoutes = $typedUnmapped
    clientRoutesWithPbToLua = $clientPbRoutes.Count
    clientPbWithoutTypeManagerCount = $clientPbWithoutTypeManager.Count
    clientPbWithoutTypeManagerRoutes = $clientPbWithoutTypeManager
} | ConvertTo-Json -Depth 3

if ($unresolved.Count -gt 0) {
    exit 1
}
if ($clientPbWithoutTypeManager.Count -gt 0) {
    exit 2
}
