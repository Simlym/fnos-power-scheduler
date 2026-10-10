$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskImages = Join-Path $taskRoot 'fpk\app\ui\images'
New-Item -ItemType Directory -Force -Path $taskImages | Out-Null
# Supersample for clean launcher icons.
function New-RoundedPath([single]$x,[single]$y,[single]$width,[single]$height,[single]$radius) {
 $path=[System.Drawing.Drawing2D.GraphicsPath]::new()
 $diameter=$radius*2
 $path.AddArc($x,$y,$diameter,$diameter,180,90)
 $path.AddArc(($x+$width-$diameter),$y,$diameter,$diameter,270,90)
 $path.AddArc(($x+$width-$diameter),($y+$height-$diameter),$diameter,$diameter,0,90)
 $path.AddArc($x,($y+$height-$diameter),$diameter,$diameter,90,90)
 $path.CloseFigure()
 return ,$path
}
foreach ($taskSize in @(64,256)) {
 $taskCanvas=[System.Drawing.Bitmap]::new(768,768)
 $taskGraphics=[System.Drawing.Graphics]::FromImage($taskCanvas)
 $taskGraphics.SmoothingMode=[System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
 $taskGraphics.ScaleTransform(3,3)
 $taskGraphics.Clear([System.Drawing.Color]::Transparent)
 $taskTile=New-RoundedPath 4 4 248 248 58
 $taskGradient=[System.Drawing.Drawing2D.LinearGradientBrush]::new([System.Drawing.Point]::new(30,12),[System.Drawing.Point]::new(218,248),[System.Drawing.ColorTranslator]::FromHtml('#55DCFA'),[System.Drawing.ColorTranslator]::FromHtml('#3269E6'))
 $taskBlend=[System.Drawing.Drawing2D.ColorBlend]::new(3)
 $taskBlend.Colors=@([System.Drawing.ColorTranslator]::FromHtml('#55DCFA'),[System.Drawing.ColorTranslator]::FromHtml('#239CF5'),[System.Drawing.ColorTranslator]::FromHtml('#3269E6'))
 $taskBlend.Positions=@([single]0,[single]0.48,[single]1)
 $taskGradient.InterpolationColors=$taskBlend
 $taskGraphics.FillPath($taskGradient,$taskTile)
 $taskGraphics.SetClip($taskTile)
 $taskShine=[System.Drawing.Drawing2D.LinearGradientBrush]::new([System.Drawing.Point]::new(0,9),[System.Drawing.Point]::new(0,150),[System.Drawing.Color]::FromArgb(65,255,255,255),[System.Drawing.Color]::FromArgb(0,255,255,255))
 $taskGraphics.FillEllipse($taskShine,-50,-60,360,200)
 $taskGraphics.ResetClip()
 $taskEdge=[System.Drawing.Pen]::new([System.Drawing.Color]::FromArgb(60,255,255,255),2)
 $taskBorder=New-RoundedPath 6 6 244 244 56
 $taskGraphics.DrawPath($taskEdge,$taskBorder)
 $taskPen=[System.Drawing.Pen]::new([System.Drawing.Color]::White,17)
 $taskPen.StartCap=$taskPen.EndCap=[System.Drawing.Drawing2D.LineCap]::Round
 $taskGraphics.DrawArc($taskPen,63,57,130,130,-51,282)
 $taskGraphics.DrawLine($taskPen,128,53,128,118)
 $taskClockBack=[System.Drawing.SolidBrush]::new([System.Drawing.ColorTranslator]::FromHtml('#247FEF'))
 $taskClockFace=[System.Drawing.SolidBrush]::new([System.Drawing.ColorTranslator]::FromHtml('#F5FCFF'))
 $taskGraphics.FillEllipse($taskClockBack,143,144,80,80)
 $taskGraphics.FillEllipse($taskClockFace,152,153,62,62)
 $taskHand=[System.Drawing.Pen]::new([System.Drawing.ColorTranslator]::FromHtml('#2687EF'),7)
 $taskHand.StartCap=$taskHand.EndCap=[System.Drawing.Drawing2D.LineCap]::Round
 $taskHand.LineJoin=[System.Drawing.Drawing2D.LineJoin]::Round
 $taskGraphics.DrawLines($taskHand,[System.Drawing.PointF[]]@([System.Drawing.PointF]::new(183,165),[System.Drawing.PointF]::new(183,184),[System.Drawing.PointF]::new(196,192)))
 $taskBitmap=[System.Drawing.Bitmap]::new($taskSize,$taskSize)
 $taskOutput=[System.Drawing.Graphics]::FromImage($taskBitmap)
 $taskOutput.InterpolationMode=[System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
 $taskOutput.DrawImage($taskCanvas,0,0,$taskSize,$taskSize)
 $taskBitmap.Save((Join-Path $taskImages "icon_$taskSize.png"),[System.Drawing.Imaging.ImageFormat]::Png)
 foreach ($taskResource in @($taskOutput,$taskBitmap,$taskHand,$taskClockFace,$taskClockBack,$taskPen,$taskBorder,$taskEdge,$taskShine,$taskGradient,$taskTile,$taskGraphics,$taskCanvas)) { $taskResource.Dispose() }
}
Copy-Item -LiteralPath "$taskImages\icon_64.png" -Destination "$taskRoot\fpk\ICON.PNG" -Force
Copy-Item -LiteralPath "$taskImages\icon_256.png" -Destination "$taskRoot\fpk\ICON_256.PNG" -Force
