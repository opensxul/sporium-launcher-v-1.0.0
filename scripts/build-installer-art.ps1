$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Drawing
$workspace=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$destination=Join-Path $workspace 'src-tauri/installer'
[void][IO.Directory]::CreateDirectory($destination)
$logo=[Drawing.Bitmap]::FromFile((Join-Path $workspace 'src-tauri/icons/generated/256x256.png'))
try {
    foreach($kind in @('sidebar','header')) {
        $width=if($kind -eq 'sidebar'){164}else{150}
        $height=if($kind -eq 'sidebar'){314}else{57}
        $canvas=New-Object Drawing.Bitmap($width,$height,[Drawing.Imaging.PixelFormat]::Format24bppRgb)
        $g=[Drawing.Graphics]::FromImage($canvas)
        try {
            $g.Clear([Drawing.ColorTranslator]::FromHtml('#0a1b16'))
            $g.InterpolationMode=[Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
            if($kind -eq 'sidebar') {
                $g.DrawImage($logo,20,30,124,124)
                $font=New-Object Drawing.Font('Segoe UI',18,[Drawing.FontStyle]::Bold)
                $brush=New-Object Drawing.SolidBrush([Drawing.ColorTranslator]::FromHtml('#fbf3df'))
                try {$g.DrawString('SPORIUM',$font,$brush,17,174)}finally{$font.Dispose();$brush.Dispose()}
                $pen=New-Object Drawing.Pen([Drawing.ColorTranslator]::FromHtml('#7eddb0'),2)
                try{$g.DrawLine($pen,22,220,142,220)}finally{$pen.Dispose()}
            } else { $g.DrawImage($logo,53,6,44,44) }
            $canvas.Save((Join-Path $destination ($kind+'.bmp')),[Drawing.Imaging.ImageFormat]::Bmp)
        }finally{$g.Dispose();$canvas.Dispose()}
    }
}finally{$logo.Dispose()}
