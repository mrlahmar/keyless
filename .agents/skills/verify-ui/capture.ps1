# Screenshots for checking Keyless UI changes.
#   capture.ps1 -List                       list windows of the running keyless.exe
#   capture.ps1 -Handle <hwnd> -Out a.png   capture one window (without its shadow)
#   capture.ps1 -Out a.png                  capture the primary screen
param([string]$Out, [long]$Handle = 0, [switch]$List)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type @"
using System; using System.Text; using System.Runtime.InteropServices; using System.Collections.Generic;
public static class KeylessCapture {
    public struct RECT { public int L, T, R, B; }
    public delegate bool EnumProc(IntPtr h, IntPtr l);
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc p, IntPtr l);
    [DllImport("user32.dll")] public static extern int GetWindowThreadProcessId(IntPtr h, out int pid);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
    [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int a, out RECT r, int s);
    public static RECT Bounds(IntPtr h) { RECT r; DwmGetWindowAttribute(h, 9, out r, 16); return r; }
    public static List<string> Windows(int pid) {
        var found = new List<string>();
        EnumWindows((h, l) => {
            int p; GetWindowThreadProcessId(h, out p);
            var title = new StringBuilder(256); GetWindowText(h, title, 256);
            if (p == pid && title.Length > 0) {
                var r = Bounds(h);
                found.Add(string.Format("{0}  visible={1}  '{2}'  {3},{4} {5}x{6}", h, IsWindowVisible(h), title, r.L, r.T, r.R - r.L, r.B - r.T));
            }
            return true;
        }, IntPtr.Zero);
        return found;
    }
}
"@
[KeylessCapture]::SetProcessDPIAware() | Out-Null

if ($List) {
    foreach ($p in Get-Process keyless) { [KeylessCapture]::Windows($p.Id) }
    return
}
if (-not $Out) { throw 'Pass -Out <file.png> or -List.' }

if ($Handle) {
    $r = [KeylessCapture]::Bounds([IntPtr]$Handle)
    $x = $r.L; $y = $r.T; $w = $r.R - $r.L; $h = $r.B - $r.T
} else {
    $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $x = $b.X; $y = $b.Y; $w = $b.Width; $h = $b.Height
}
if ($w -le 0 -or $h -le 0) { throw "Window $Handle is not visible." }
$bmp = New-Object System.Drawing.Bitmap $w, $h
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($x, $y, 0, 0, $bmp.Size)
$bmp.Save([System.IO.Path]::GetFullPath($Out, (Get-Location).Path))
$g.Dispose(); $bmp.Dispose()
"Saved $Out ($w x $h at $x,$y)"
