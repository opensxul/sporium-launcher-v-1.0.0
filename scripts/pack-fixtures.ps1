param([string]$Directory)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
[IO.Directory]::CreateDirectory($Directory) | Out-Null
$taskFiles=@(
    @{ path='mods/optional.jar'; hashes=@{sha1=('0'*40);sha512=('0'*128)}; downloads=@('https://cdn.modrinth.com/optional.jar'); fileSize=12; env=@{client='optional';server='required'} },
    @{ path='mods/server.jar'; hashes=@{sha1=('0'*40);sha512=('0'*128)}; downloads=@('https://cdn.modrinth.com/server.jar'); fileSize=12; env=@{client='unsupported';server='required'} }
)
$taskManifest = @{ formatVersion=1; game='minecraft'; versionId='fixture-1'; name='Native import fixture'; summary='Import smoke fixture'; dependencies=@{ minecraft='1.21.1'; 'fabric-loader'='0.16.14' }; files=$taskFiles } | ConvertTo-Json -Depth 8 -Compress
$taskArchives = @{
    'valid.mrpack'=@{ 'modrinth.index.json'=$taskManifest; 'overrides/options.txt'='fixture:shared'; 'client-overrides/options.txt'='fixture:client'; 'server-overrides/config/server-only.txt'='must not install'; 'overrides/saves/World/level.dat'='preserved world fixture'; 'overrides/config/settings.txt'='setting fixture' }
    'unsafe.mrpack'=@{ 'modrinth.index.json'=$taskManifest; 'overrides/../escape.txt'='unsafe' }
    'corrupt.mrpack'=@{ 'modrinth.index.json'='{"formatVersion":999}' }
}
foreach ($taskName in $taskArchives.Keys) {
    $taskZip=[IO.Compression.ZipFile]::Open((Join-Path $Directory $taskName),[IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($taskEntry in $taskArchives[$taskName].Keys) {
            $taskWriter=[IO.StreamWriter]::new($taskZip.CreateEntry($taskEntry).Open(),[Text.UTF8Encoding]::new($false))
            try { $taskWriter.Write($taskArchives[$taskName][$taskEntry]) } finally { $taskWriter.Dispose() }
        }
    } finally { $taskZip.Dispose() }
}
$taskPrism=Join-Path $Directory 'Prism'
[IO.Directory]::CreateDirectory((Join-Path $taskPrism '.minecraft/config')) | Out-Null
[IO.File]::WriteAllText((Join-Path $taskPrism 'instance.cfg'),"name=Native Prism copy`nPreLaunchCommand=do-not-import`n",[Text.UTF8Encoding]::new($false))
[IO.File]::WriteAllText((Join-Path $taskPrism 'mmc-pack.json'),' {"formatVersion":1,"components":[{"uid":"net.minecraft","version":"1.21.1"},{"uid":"net.fabricmc.fabric-loader","version":"0.16.14"}]}',[Text.UTF8Encoding]::new($false))
[IO.File]::WriteAllText((Join-Path $taskPrism '.minecraft/config/test.txt'),'prism config',[Text.UTF8Encoding]::new($false))
[IO.File]::WriteAllText((Join-Path $taskPrism '.minecraft/accounts.json'),'secret fixture',[Text.UTF8Encoding]::new($false))
