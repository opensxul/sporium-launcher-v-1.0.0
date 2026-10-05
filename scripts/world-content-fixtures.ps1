param([string]$Directory)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
[System.IO.Directory]::CreateDirectory($Directory) | Out-Null
$taskArchives = @{
    'map.zip' = @{'Example/level.dat'='imported world fixture'; 'Example/region/r.0.0.mca'='preserved region fixture'}
    'datapack.zip' = @{'pack.mcmeta'='{"pack":{"pack_format":48,"description":"Sporium test"}}'; 'data/sporium/function/test.mcfunction'='say Sporium test'}
}
foreach ($taskName in $taskArchives.Keys) {
    $taskArchive = [System.IO.Compression.ZipFile]::Open((Join-Path $Directory $taskName), [System.IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($taskEntry in $taskArchives[$taskName].Keys) {
            $taskWriter = [System.IO.StreamWriter]::new($taskArchive.CreateEntry($taskEntry).Open(), [System.Text.UTF8Encoding]::new($false))
            try { $taskWriter.Write($taskArchives[$taskName][$taskEntry]) } finally { $taskWriter.Dispose() }
        }
    } finally { $taskArchive.Dispose() }
}
