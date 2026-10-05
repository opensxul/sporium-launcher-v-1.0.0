param([string]$Directory)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
[System.IO.Directory]::CreateDirectory($Directory) | Out-Null
$taskPng = [Convert]::FromBase64String('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRzUAAAAASUVORK5CYII=')
foreach ($taskName in @('manual.jar','unknown.jar','pack.zip')) {
    $taskArchive = [System.IO.Compression.ZipFile]::Open((Join-Path $Directory $taskName), [System.IO.Compression.ZipArchiveMode]::Create)
    try {
        if ($taskName -eq 'manual.jar') {
            $taskMetadata = @{schemaVersion=1;id='adoption_manual';name='Manual adoption';version='1.0';environment='client';icon='assets/icon.png';depends=@{minecraft='1.21.1'}} | ConvertTo-Json -Compress
            $taskMetadataName = 'fabric.mod.json'
            $taskIconName = 'assets/icon.png'
        } elseif ($taskName -eq 'pack.zip') {
            $taskMetadata = '{"pack":{"pack_format":34,"description":"Local icon pack"}}'
            $taskMetadataName = 'pack.mcmeta'
            $taskIconName = 'pack.png'
        } else {
            $taskMetadata = 'Unidentified archive'
            $taskMetadataName = 'readme.txt'
            $taskIconName = ''
        }
        $taskWriter = [System.IO.StreamWriter]::new($taskArchive.CreateEntry($taskMetadataName).Open(), [System.Text.UTF8Encoding]::new($false))
        try { $taskWriter.Write($taskMetadata) } finally { $taskWriter.Dispose() }
        if ($taskIconName) {
            $taskStream = $taskArchive.CreateEntry($taskIconName).Open()
            try { $taskStream.Write($taskPng, 0, $taskPng.Length) } finally { $taskStream.Dispose() }
        }
    } finally { $taskArchive.Dispose() }
}
