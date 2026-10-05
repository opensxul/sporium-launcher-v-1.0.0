param([string]$Destination = '')
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
$sourceRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (-not (Test-Path -LiteralPath (Join-Path $sourceRoot 'src-tauri\Cargo.toml'))) { throw 'Sporium source is incomplete' }
if (-not $Destination) {
    $backupRoot = Join-Path ([Environment]::GetFolderPath('MyDocuments')) 'Sporium Backups'
    [void][IO.Directory]::CreateDirectory($backupRoot)
    $Destination = Join-Path $backupRoot ('Sporium-source-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '.zip')
}
$Destination = [IO.Path]::GetFullPath($Destination)
$files = @()
foreach ($name in @('src','src-tauri','docs','scripts','public','tests')) {
    $directory = Join-Path $sourceRoot $name
    $files += Get-ChildItem -LiteralPath $directory -Recurse -File | Where-Object {
        $relative = $_.FullName.Substring($sourceRoot.Length + 1).Replace('\','/')
        -not ($relative -match '^src-tauri/(target|gen)/')
    }
}
$files += Get-ChildItem -LiteralPath $sourceRoot -File | Where-Object {
    $_.Name -match '\.(json|ts|js|html|md|cmd)$' -or $_.Name -in @('.gitignore','.gitattributes','.prettierignore')
}
$stream = [IO.File]::Open($Destination, [IO.FileMode]::CreateNew, [IO.FileAccess]::ReadWrite)
try {
    $zip = New-Object IO.Compression.ZipArchive($stream, [IO.Compression.ZipArchiveMode]::Create, $true)
    try {
        foreach ($file in $files) {
            $resolved = [IO.Path]::GetFullPath($file.FullName)
            if (-not $resolved.StartsWith($sourceRoot + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Unexpected source path' }
            $relative = $resolved.Substring($sourceRoot.Length + 1).Replace('\','/')
            [void][IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, $resolved, $relative, [IO.Compression.CompressionLevel]::Optimal)
        }
    } finally { $zip.Dispose() }
} finally { $stream.Dispose() }
$check = [IO.Compression.ZipFile]::OpenRead($Destination)
try {
    foreach ($required in @('src-tauri/Cargo.toml','src-tauri/Cargo.lock','src-tauri/src/lib.rs','src/main.tsx','README.md')) {
        if (-not $check.GetEntry($required)) { throw ('Archive is missing ' + $required) }
    }
    Write-Output ('Verified source backup: ' + $Destination + ' (' + $check.Entries.Count + ' files)')
} finally { $check.Dispose() }
