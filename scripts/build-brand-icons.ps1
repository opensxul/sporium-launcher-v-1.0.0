param([string]$Source = '')
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$workspace = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$masterDirectory = Join-Path $workspace 'src-tauri/icons/master'
$outputDirectory = Join-Path $workspace 'src-tauri/icons/generated'
$webDirectory = Join-Path $workspace 'public/branding'
foreach ($directory in @($masterDirectory, $outputDirectory, $webDirectory)) { [void][IO.Directory]::CreateDirectory($directory) }
$masterPath = Join-Path $masterDirectory 'sporium-master.png'
if ($Source) { Copy-Item -LiteralPath $Source -Destination $masterPath }
$master = [Drawing.Bitmap]::FromFile($masterPath)
try {
    if ($master.Width -ne $master.Height) { throw 'Master artwork must be square; do not crop or pad it automatically.' }
    $cornerAlpha = $master.GetPixel(0,0).A
    if ($cornerAlpha -ne 0) { throw 'Expected transparent master artwork; do not remove its background automatically.' }
    $sizes = @(16,24,32,48,64,128,256)
    $frames = @()
    foreach ($size in $sizes) {
        $bitmap = New-Object Drawing.Bitmap($size, $size, [Drawing.Imaging.PixelFormat]::Format32bppArgb)
        try {
            $graphics = [Drawing.Graphics]::FromImage($bitmap)
            try {
                $graphics.Clear([Drawing.Color]::Transparent)
                $graphics.CompositingMode = [Drawing.Drawing2D.CompositingMode]::SourceCopy
                $graphics.CompositingQuality = [Drawing.Drawing2D.CompositingQuality]::HighQuality
                $graphics.InterpolationMode = [Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
                $graphics.PixelOffsetMode = [Drawing.Drawing2D.PixelOffsetMode]::HighQuality
                $attributes = New-Object Drawing.Imaging.ImageAttributes
                try {
                    $attributes.SetWrapMode([Drawing.Drawing2D.WrapMode]::TileFlipXY)
                    $graphics.DrawImage($master, [Drawing.Rectangle]::new(0,0,$size,$size), 0,0,$master.Width,$master.Height,[Drawing.GraphicsUnit]::Pixel,$attributes)
                } finally { $attributes.Dispose() }
            } finally { $graphics.Dispose() }
            $stream = New-Object IO.MemoryStream
            try { $bitmap.Save($stream,[Drawing.Imaging.ImageFormat]::Png); $bytes=$stream.ToArray() } finally { $stream.Dispose() }
            [IO.File]::WriteAllBytes((Join-Path $outputDirectory ($size.ToString()+'x'+$size+'.png')), $bytes)
            $frames += ,$bytes
            if ($size -in @(32,128,256)) { [IO.File]::WriteAllBytes((Join-Path $webDirectory ('mark-'+$size+'.png')), $bytes) }
        } finally { $bitmap.Dispose() }
    }
    $icoPath = Join-Path $outputDirectory 'icon.ico'
    $stream = [IO.File]::Create($icoPath)
    $writer = New-Object IO.BinaryWriter($stream)
    try {
        $writer.Write([uint16]0); $writer.Write([uint16]1); $writer.Write([uint16]$sizes.Count)
        $offset = 6 + 16 * $sizes.Count
        for ($i=0; $i -lt $sizes.Count; $i++) {
            $dimension = if ($sizes[$i] -eq 256) { 0 } else { $sizes[$i] }
            $writer.Write([byte]$dimension); $writer.Write([byte]$dimension)
            $writer.Write([byte]0); $writer.Write([byte]0)
            $writer.Write([uint16]1); $writer.Write([uint16]32)
            $writer.Write([uint32]$frames[$i].Length); $writer.Write([uint32]$offset)
            $offset += $frames[$i].Length
        }
        foreach ($frame in $frames) { $writer.Write([byte[]]$frame) }
    } finally { $writer.Dispose(); $stream.Dispose() }
    $sha = [Security.Cryptography.SHA256]::Create()
    try { $hash = [BitConverter]::ToString($sha.ComputeHash([IO.File]::ReadAllBytes($masterPath))).Replace('-','') } finally { $sha.Dispose() }
    Write-Output ('Preserved master: '+$master.Width+'x'+$master.Height+', corner alpha='+$cornerAlpha+', SHA256='+$hash)
    Write-Output ('Generated PNG/ICO sizes: '+($sizes -join ', ')+'; UI derivatives: 32, 128, 256')
} finally { $master.Dispose() }
