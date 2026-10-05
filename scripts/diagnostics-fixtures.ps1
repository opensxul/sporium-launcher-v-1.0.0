param([string]$Directory)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
[System.IO.Directory]::CreateDirectory($Directory) | Out-Null
$taskMods = @(
    @{file='owner.jar'; id='diagnostic_owner'; name='Dependency probe'; version='1.0'; depends=@{'fabric-api'='*'; disabled_lib='>=1'; wrong_lib='>=2'}; breaks=@{bad_lib='*'}; conflicts=@{soft_lib='*'}; suggests=@{optional_lib='>=2'}},
    @{file='disabled_lib.jar'; id='disabled_lib'; version='1.0'},
    @{file='wrong_lib.jar'; id='wrong_lib'; version='1.0'},
    @{file='wrong_lib_fixed.jar'; id='wrong_lib'; version='2.0'},
    @{file='bad_lib.jar'; id='bad_lib'; version='1.0'},
    @{file='soft_lib.jar'; id='soft_lib'; version='1.0'},
    @{file='api.jar'; id='fabric-api'; version='1.0'},
    @{file='nested.jar'; id='nested_probe'; version='1.0'; jars=@(@{file='uninspected.jar'})},
    @{file='staged.jar'; id='staged_probe'; version='1.0'; depends=@{staged_dependency='>=2'}},
    @{file='staged_dependency.jar'; id='staged_dependency'; version='2.0'}
)
foreach ($taskMod in $taskMods) {
    $taskMetadata = @{schemaVersion=1; environment='client'; depends=@{minecraft='1.21.1'}}
    foreach ($taskKey in $taskMod.Keys) { if ($taskKey -ne 'file') { $taskMetadata[$taskKey] = $taskMod[$taskKey] } }
    $taskMetadata.depends['minecraft'] = '1.21.1'
    $taskArchive = [System.IO.Compression.ZipFile]::Open((Join-Path $Directory $taskMod.file), [System.IO.Compression.ZipArchiveMode]::Create)
    try {
        $taskWriter = [System.IO.StreamWriter]::new($taskArchive.CreateEntry('fabric.mod.json').Open(), [System.Text.UTF8Encoding]::new($false))
        try { $taskWriter.Write(($taskMetadata | ConvertTo-Json -Depth 6 -Compress)) } finally { $taskWriter.Dispose() }
    } finally { $taskArchive.Dispose() }
}
