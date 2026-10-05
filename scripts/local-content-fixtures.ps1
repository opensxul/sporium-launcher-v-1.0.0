param([string]$Directory)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
[System.IO.Directory]::CreateDirectory($Directory) | Out-Null
foreach ($name in @('picked','drop-one','drop-two','wrong-version','unknown')) {
    $file = Join-Path $Directory ($name + '.jar')
    $archive = [System.IO.Compression.ZipFile]::Open($file, [System.IO.Compression.ZipArchiveMode]::Create)
    try {
        $entry = $archive.CreateEntry($(if ($name -eq 'unknown') {'readme.txt'} else {'fabric.mod.json'}))
        $writer = New-Object System.IO.StreamWriter($entry.Open(), (New-Object System.Text.UTF8Encoding($false)))
        try {
            if ($name -eq 'unknown') { $writer.Write('Unidentified local fixture') }
            else { $writer.Write((@{schemaVersion=1;id=('test_'+$name.Replace('-','_'));name=('Local '+$name);version='1.0';environment='client';depends=@{minecraft=$(if($name -eq 'wrong-version'){'1.20.1'}else{'1.21.1'})}} | ConvertTo-Json -Compress)) }
        } finally { $writer.Dispose() }
    } finally { $archive.Dispose() }
}
