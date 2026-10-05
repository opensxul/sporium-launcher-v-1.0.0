$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$version = (Get-Content -LiteralPath (Join-Path $root 'src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json).version
$testRoot = [IO.Path]::GetFullPath((Join-Path $root '.local\installer-smoke'))
$install = [IO.Path]::GetFullPath((Join-Path $testRoot ([Guid]::NewGuid().ToString())))
if (-not $install.StartsWith($testRoot + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe test directory' }
$uninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Sporium'
$productKey = 'HKCU:\Software\sporium\Sporium'
foreach ($key in @($uninstallKey, $productKey, 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Sporium')) {
    if (Test-Path -LiteralPath $key) { throw 'An existing Sporium installation must not be changed by this test' }
}
if (Get-Process Sporium -ErrorAction SilentlyContinue) { throw 'Close Sporium before installer verification' }
[void][IO.Directory]::CreateDirectory($install)
$data = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'app.sporium.launcher'
[void][IO.Directory]::CreateDirectory($data)
$sentinel = Join-Path $data ('.installer-smoke-' + [Guid]::NewGuid().ToString() + '.txt')
$payload = 'Installer must preserve independent launcher data.'
[IO.File]::WriteAllText($sentinel, $payload)
$database = Join-Path $data 'launcher\sporium.sqlite3'
function Digest([string]$file, [bool]$NormalizeBundle = $false) {
    if (-not [IO.File]::Exists($file)) { return 'absent' }
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [IO.File]::ReadAllBytes($file)
        if ($NormalizeBundle) {
            # Tauri embeds NSS in the packaged EXE, then restores UNK in the build output.
            $marker = '__TAURI_BUNDLE_TYPE_VAR_UNK'
            $offset = [Text.Encoding]::GetEncoding(28591).GetString($bytes).IndexOf($marker, [StringComparison]::Ordinal)
            if ($offset -ge 0) { [Text.Encoding]::ASCII.GetBytes('NSS').CopyTo($bytes, $offset + $marker.Length - 3) }
        }
        return [BitConverter]::ToString($sha.ComputeHash($bytes))
    }
    finally { $sha.Dispose() }
}
$databaseBefore = Digest $database
$installer = Join-Path $root 'releases\SporiumLauncher.exe'
$expected = Digest (Join-Path $root 'src-tauri\target\release\Sporium.exe') $true
try {
    for ($attempt = 0; $attempt -lt 2; $attempt++) {
        $process = Start-Process -FilePath $installer -ArgumentList ('/S /NS /D=' + $install) -WindowStyle Hidden -PassThru -Wait
        if ($process.ExitCode -ne 0) { throw ('Installer failed: ' + $process.ExitCode) }
        if ((Digest (Join-Path $install 'Sporium.exe') $true) -ne $expected) { throw 'Installed executable differs from release build' }
        if ((Get-ItemProperty -LiteralPath $uninstallKey).DisplayVersion -ne $version) { throw 'Incorrect installed version' }
        if (Test-Path -LiteralPath (Join-Path $install 'export-bindings.exe')) { throw 'Developer tool leaked into installer' }
        if (Test-Path -LiteralPath (Join-Path $install 'game-probe.exe')) { throw 'Developer tool leaked into installer' }
        if ([IO.File]::ReadAllText($sentinel) -ne $payload -or (Digest $database) -ne $databaseBefore) { throw 'Installation modified launcher data' }
    }
    $process = Start-Process -FilePath (Join-Path $install 'uninstall.exe') -ArgumentList ('/S _?=' + $install) -WindowStyle Hidden -PassThru -Wait
    if ($process.ExitCode -ne 0) { throw ('Uninstaller failed: ' + $process.ExitCode) }
    if (Test-Path -LiteralPath (Join-Path $install 'Sporium.exe')) { throw 'Uninstall left main executable' }
    if (Test-Path -LiteralPath $uninstallKey) { throw 'Uninstall left application registration' }
    if ([IO.File]::ReadAllText($sentinel) -ne $payload -or (Digest $database) -ne $databaseBefore) { throw 'Uninstall modified launcher data' }
    # The stock uninstaller retains the installation path when user data is preserved.
    # This key did not exist before the test and points only to our checked fixture path.
    if (Test-Path -LiteralPath $productKey) {
        if ((Get-Item -LiteralPath $productKey).GetValue('') -ne $install) { throw 'Unexpected product registration; retain it for inspection' }
        Remove-Item -LiteralPath $productKey
    }
    $result = @{ passed = $true; installPath = $install; version = $version; checks = @('silent installation', 'same-version reinstall', 'release byte identity', 'no developer executables', 'silent uninstall', 'user data and SQLite preserved') }
    $result | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $testRoot 'result.json') -Encoding UTF8
    Write-Output 'Installer checks passed: install, reinstall, uninstall, preserved data, release identity.'
} finally {
    if ([IO.File]::Exists($sentinel)) { [IO.File]::Delete($sentinel) }
}
