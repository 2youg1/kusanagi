# Capture the real kusanagi window as the GPU drew it (Direct2D + DirectWrite),
# not the CPU reference renderer that `native automate screenshot` uses.
# Usage: powershell -NoProfile -File real.ps1 shots/<name>.png
param([string]$Out = "shots/real.png")
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public class W {
  [DllImport("user32.dll")] public static extern IntPtr FindWindow(string c, string t);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out R r);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref P p);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
  [StructLayout(LayoutKind.Sequential)] public struct R { public int L, T, Rt, B; }
  [StructLayout(LayoutKind.Sequential)] public struct P { public int X, Y; }
}
"@
$proc = Get-Process glass -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $proc) { Write-Error "glass is not running with a window"; exit 1 }
$h = $proc.MainWindowHandle
# A background process may not take the foreground, but it may pin its own
# window above the others for the length of one capture.
$topmost = [IntPtr](-1); $notopmost = [IntPtr](-2); $flags = [uint32](0x0001 -bor 0x0002 -bor 0x0040)
[W]::SetWindowPos($h, $topmost, 0, 0, 0, 0, $flags) | Out-Null
Start-Sleep -Milliseconds 400
$r = New-Object W+R; [W]::GetClientRect($h, [ref]$r) | Out-Null
$p = New-Object W+P; [W]::ClientToScreen($h, [ref]$p) | Out-Null
$w = $r.Rt - $r.L; $ht = $r.B - $r.T
$bmp = New-Object System.Drawing.Bitmap $w, $ht
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($p.X, $p.Y, 0, 0, (New-Object System.Drawing.Size $w, $ht))
$bmp.Save((Join-Path (Get-Location) $Out), [System.Drawing.Imaging.ImageFormat]::Png)
[W]::SetWindowPos($h, $notopmost, 0, 0, 0, 0, $flags) | Out-Null
Write-Output $Out
