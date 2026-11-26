[CmdletBinding()]
param([switch] $Check, [switch] $Warm,
    [string] $GameDirectory)
$ErrorActionPreference = 'Stop'
if (![IO.File]::Exists((Join-Path $PSScriptRoot 'victory.exe'))) {
    throw 'Extract the Windows release before starting this launcher.'
}
$config = Join-Path $PSScriptRoot 'game-directory.txt'
if (!$GameDirectory -and [IO.File]::Exists($config)) { $GameDirectory = [IO.File]::ReadAllText($config).Trim() }
if (!$GameDirectory) {
    Add-Type -AssemblyName System.Windows.Forms
    $picker = [Windows.Forms.OpenFileDialog]::new()
    try {
        $picker.Title = 'Choose the Series X game executable'
        $picker.Filter = 'Game executable (*.exe)|*.exe'
        if ($picker.ShowDialog() -ne [Windows.Forms.DialogResult]::OK) { return }
        $GameDirectory = [IO.Path]::GetDirectoryName($picker.FileName)
    } finally { $picker.Dispose() }
}
& (Join-Path $PSScriptRoot 'Initialize-FormatTables.ps1') -Directory $PSScriptRoot
$source = (Get-Item -LiteralPath $GameDirectory).FullName.TrimEnd('\')
$title = [IO.Directory]::CreateDirectory((Join-Path $PSScriptRoot 'title')).FullName
if ($source -eq $title -or $title.StartsWith($source + '\',[StringComparison]::OrdinalIgnoreCase)) {
    throw 'The private game folder must be outside the original game.'
}
$handles = [Collections.Generic.List[IO.FileStream]]::new()
if (-not ('VictoryFileLinks' -as [type])) {
    Add-Type -TypeDefinition @'
using System.Runtime.InteropServices;
public static class VictoryFileLinks {
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.U1)]
    public static extern bool CreateSymbolicLinkW(string name, string target, uint flags);
}
'@
}
try {
    $originals = @(Get-ChildItem -LiteralPath $source -Recurse -File -Force)
    foreach ($file in $originals) {
        $handles.Add([IO.File]::Open($file.FullName,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read))
    }
    $linked = 0
    foreach ($file in $originals) {
        $relative = $file.FullName.Substring($source.Length + 1)
        $path = Join-Path $title $relative
        [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($path))
        $item = Get-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
        if ($file.Extension -in '.rpf','.ttpf','.xvd','.alignmentchunk') {
            if (!$item) {
                if (![VictoryFileLinks]::CreateSymbolicLinkW($path, $file.FullName, 2)) {
                    throw [ComponentModel.Win32Exception]::new([Runtime.InteropServices.Marshal]::GetLastWin32Error())
                }
                $item = Get-Item -LiteralPath $path -Force
            }
            if ($item.LinkType -ne 'SymbolicLink' -or [IO.Path]::GetFullPath([string]($item.Target | Select-Object -First 1)) -ne $file.FullName) {
                throw "Archive link does not match the original file $relative."
            }
            $linked++
        } else {
            if ($item -and ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
                throw "Expected a private copy of $relative."
            }
            if (!$item -or $item.Length -ne $file.Length -or $item.LastWriteTimeUtc -ne $file.LastWriteTimeUtc) {
                [IO.File]::Copy($file.FullName, $path, $true)
            }
        }
    }
    $game = Join-Path $title 'game_scarlett_master.exe'
    $env:VICTORY_GAME_DIR = $title
    $env:VICTORY_CACHE_DIR = Join-Path $PSScriptRoot 'cache'
    $env:VICTORY_PARALLEL_PRELUDES = '1'
    $app = if ($Warm) { 'victory-warm.exe' } else { 'victory-play.exe' }
    $launcher = Join-Path $PSScriptRoot $app
    foreach ($required in @($game,$launcher,(Join-Path $PSScriptRoot 'victory.exe'))) {
        if (![IO.File]::Exists($required)) { throw "Missing file $required" }
    }
    [IO.File]::WriteAllText($config,$source,[Text.UTF8Encoding]::new($false))
    if ($Check) {
        $shaderDir = Join-Path $env:VICTORY_CACHE_DIR 'shaders'
        $shaders = if (Test-Path -LiteralPath $shaderDir) { (Get-ChildItem -LiteralPath $shaderDir -File -Recurse | Measure-Object).Count } else { 0 }
        Write-Output ("{0} original files locked for reading, {1} archive links validated, {2} private copies, {3} cached shader entries." -f $handles.Count,$linked,($handles.Count-$linked),$shaders)
    } else {
        $process = Start-Process -FilePath $launcher -ArgumentList @('--rt',('"{0}"' -f $game)) -WorkingDirectory $PSScriptRoot -WindowStyle Hidden -Wait -PassThru
        if ($process.ExitCode -ne 0) { throw "VIctory exited with code $($process.ExitCode). See the logs folder beside this launcher." }
    }
} finally {
    foreach ($handle in $handles) { $handle.Dispose() }
}
