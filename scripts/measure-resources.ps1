# SHELL-03 P7: read-only sampling of an ALREADY RUNNING Release FastNote.
# Does not launch/close an app, send messages/keys, move the mouse, take screenshots,
# change security labels, apply memory pressure, or trim the working set.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateRange(1, 2147483647)]
    [int]$AppPid,
    [ValidateRange(1, 7200)]
    [int]$Seconds = 600,
    [ValidateRange(1, 60)]
    [int]$IntervalSeconds = 5,
    [ValidatePattern('^[a-zA-Z0-9_-]{1,48}$')]
    [string]$Scenario = 'cold-idle',
    [string]$OutputDirectory = (Join-Path $PSScriptRoot '..\target\resource-measurements')
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$process = Get-Process -Id $AppPid
if ($process.ProcessName -ne 'fastnote-app') {
    throw 'AppPid must identify FastNote, not cargo/rustc/PowerShell.'
}
$started = $process.StartTime.ToUniversalTime()
$exe = $process.MainModule.FileName
if ($exe -notmatch '[\\/]release[\\/]') {
    throw 'Resource acceptance requires a Release executable. Debug is not comparable.'
}
$hash = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash
$directory = [IO.Path]::GetFullPath($OutputDirectory)
[IO.Directory]::CreateDirectory($directory) | Out-Null
$stamp = [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss-fffffff')
$output = Join-Path $directory "$stamp-$AppPid-$Scenario.csv"

if (-not ('FastNoteResourceReadOnly' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class FastNoteResourceReadOnly {
    private delegate bool EnumWindow(IntPtr window, IntPtr parameter);
    [DllImport("user32.dll")] private static extern bool EnumWindows(EnumWindow callback, IntPtr parameter);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetWindowTextW(IntPtr window, StringBuilder text, int count);
    [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr window);
    [DllImport("kernel32.dll")] private static extern IntPtr OpenProcess(uint access, bool inherit, uint process);
    [DllImport("kernel32.dll")] private static extern bool CloseHandle(IntPtr handle);
    [DllImport("user32.dll")] private static extern uint GetGuiResources(IntPtr process, uint flags);
    // Result: main HWNDs, visible main HWNDs, ball HWNDs, menu HWNDs, GDI, USER.
    public static long[] Read(uint process) {
        var result = new long[] {0,0,0,0,-1,-1};
        EnumWindow callback = (window, unused) => {
            uint owner;
            GetWindowThreadProcessId(window, out owner);
            if (owner != process) return true;
            var title = new StringBuilder(64);
            GetWindowTextW(window, title, title.Capacity);
            // Do not export arbitrary window titles or user content.
            if (title.ToString() == "FastNote") {
                result[0]++;
                if (IsWindowVisible(window)) result[1]++;
            } else if (title.ToString() == "FastNote \u60ac\u6d6e\u7403") result[2]++;
            else if (title.ToString() == "FastNote \u5165\u53e3\u83dc\u5355") result[3]++;
            return true;
        };
        if (!EnumWindows(callback, IntPtr.Zero)) throw new InvalidOperationException("Window enumeration unavailable");
        GC.KeepAlive(callback);
        var handle = OpenProcess(0x0400, false, process); // query only, no VM write or set information
        if (handle != IntPtr.Zero) {
            try { result[4] = GetGuiResources(handle, 0); result[5] = GetGuiResources(handle, 1); }
            finally { CloseHandle(handle); }
        }
        return result;
    }
}
'@
}

$watch = [Diagnostics.Stopwatch]::StartNew()
$previousCpu = $process.TotalProcessorTime.TotalSeconds
$previousElapsed = 0.0
$first = $true
Write-Host "Sampling existing PID $AppPid. Window operations remain manual. Output: $output"
while ($true) {
    try { $process = Get-Process -Id $AppPid } catch { Write-Warning 'Selected process exited; sampling stopped.'; break }
    if ($process.StartTime.ToUniversalTime() -ne $started) { throw 'PID was reused; refusing to sample another process.' }
    $process.Refresh()
    $elapsed = $watch.Elapsed.TotalSeconds
    $cpu = $process.TotalProcessorTime.TotalSeconds
    $percent = if ($elapsed -gt $previousElapsed) {
        100.0 * ($cpu - $previousCpu) / ($elapsed - $previousElapsed) / [Environment]::ProcessorCount
    } else { 0.0 }
    $native = [FastNoteResourceReadOnly]::Read([uint32]$AppPid)
    $row = [pscustomobject][ordered]@{
        TimeUtc = [DateTime]::UtcNow.ToString('o')
        ProcessStartUtc = $started.ToString('o')
        Pid = $AppPid
        Scenario = $Scenario
        ExeSha256 = $hash
        ElapsedSeconds = [Math]::Round($elapsed, 3)
        WorkingSetBytes = $process.WorkingSet64
        PrivateBytes = $process.PrivateMemorySize64
        HandleCount = $process.HandleCount
        GdiObjects = $native[4]
        UserObjects = $native[5]
        MainHwndCount = $native[0]
        VisibleMainHwndCount = $native[1]
        BallHwndCount = $native[2]
        MenuHwndCount = $native[3]
        CpuPercent = if ($first) { $null } else { [Math]::Round($percent, 4) }
        # GPU allocation is not exposed by these APIs; blank is not a measured zero.
        GpuDedicatedBytes = $null
        GpuSharedBytes = $null
    }
    if ($first) {
        $row | Export-Csv -LiteralPath $output -NoTypeInformation -Encoding UTF8 -NoClobber
        $first = $false
    } else {
        $row | Export-Csv -LiteralPath $output -NoTypeInformation -Encoding UTF8 -Append
    }
    $previousCpu = $cpu
    $previousElapsed = $elapsed
    if ($elapsed -ge $Seconds) { break }
    Start-Sleep -Milliseconds ([int]([Math]::Min($IntervalSeconds, $Seconds - $elapsed) * 1000))
}
Write-Host "Read-only sampling complete: $output (MB = 1,000,000 bytes; missing counters are not zero)."
