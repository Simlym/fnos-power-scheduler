$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Drawing
$taskRoot=Split-Path -Parent $PSScriptRoot
$taskImages=Join-Path $taskRoot 'fpk\app\ui\images'
New-Item -ItemType Directory -Force -Path $taskImages | Out-Null
foreach ($taskSize in @(64,256)) {
 $taskBitmap=[System.Drawing.Bitmap]::new($taskSize,$taskSize)
 $taskGraphics=[System.Drawing.Graphics]::FromImage($taskBitmap)
 $taskGraphics.SmoothingMode=[System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
 $taskGraphics.Clear([System.Drawing.Color]::FromArgb(32,120,91))
 $taskPen=[System.Drawing.Pen]::new([System.Drawing.Color]::White,[single]($taskSize*0.065))
 $taskPen.StartCap=[System.Drawing.Drawing2D.LineCap]::Round
 $taskPen.EndCap=[System.Drawing.Drawing2D.LineCap]::Round
 $taskGraphics.DrawArc($taskPen,[single]($taskSize*0.23),[single]($taskSize*0.25),[single]($taskSize*0.54),[single]($taskSize*0.54),[single](-48),[single](276))
 $taskGraphics.DrawLine($taskPen,[single]($taskSize*0.5),[single]($taskSize*0.19),[single]($taskSize*0.5),[single]($taskSize*0.49))
 $taskBitmap.Save((Join-Path $taskImages "icon_$taskSize.png"),[System.Drawing.Imaging.ImageFormat]::Png)
 $taskPen.Dispose();$taskGraphics.Dispose();$taskBitmap.Dispose()
}
Copy-Item -LiteralPath "$taskImages\icon_64.png" -Destination "$taskRoot\fpk\ICON.PNG" -Force
Copy-Item -LiteralPath "$taskImages\icon_256.png" -Destination "$taskRoot\fpk\ICON_256.PNG" -Force
