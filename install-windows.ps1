# MusicZero Windows installer.
#
# Copies a packaged musiczero folder (mz.exe + modules\<id>\) into
# %LOCALAPPDATA%\MusicZero, adds it to the user PATH and can later undo
# everything. Works with release archives and with dist\musiczero from a
# repository checkout.
#
# Usage:
#   .\install-windows.ps1                # install or update
#   .\install-windows.ps1 -Uninstall    # remove files, PATH entry and shortcut
#   .\install-windows.ps1 -Check        # verify the package layout and exit
#
# Options:
#   -Source <dir>     musiczero folder to install from (default: next to the
#                     script); supports a zip archive path as well
#   -Shortcut         create a Start Menu shortcut that starts local music
param(
    [switch]$Uninstall,
    [switch]$Check,
    [switch]$Shortcut,
    [string]$Source
)

$ErrorActionPreference = 'Stop'

$InstallDir = Join-Path $env:LOCALAPPDATA 'MusicZero'
$StartMenu = [Environment]::GetFolderPath('Programs')
$ShortcutPath = Join-Path $StartMenu 'MusicZero.lnk'
$PathEntry = $InstallDir

function Expand-ZipArchive([string]$Zip, [string]$Destination) {
    # Compress-Archive uses backslashes in entry names, which .NET
    # ZipFile::ExtractToDirectory handles; plain extraction keeps structure.
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [System.IO.Compression.ZipFile]::ExtractToDirectory($Zip, $Destination, $true)
}

function Assert-Package([string]$Root) {
    foreach ($required in @('mz.exe', 'modules\ymz\module.json')) {
        $path = Join-Path $Root $required
        if (-not (Test-Path $path)) {
            throw "Packaging is incomplete: '$required' is missing under $Root"
        }
    }
}

function Backup-Previous([string]$Directory) {
    # User data lives outside the folder; nothing to keep except settings
    # copied by the host itself into %APPDATA%. Old binaries are replaced.
    if (Test-Path $Directory) {
        Remove-Item $Directory -Recurse -Force
    }
}

if ($Uninstall) {
    if (Test-Path $InstallDir) {
        Remove-Item $InstallDir -Recurse -Force
        Write-Host "Deleted $InstallDir"
    }
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $kept = ($userPath -split ';' | Where-Object { $_ -and $_ -ne $PathEntry }) -join ';'
    [Environment]::SetEnvironmentVariable('Path', $kept, 'User')
    if (Test-Path $ShortcutPath) {
        Remove-Item $ShortcutPath -Force
        Write-Host "Deleted $ShortcutPath"
    }
    Write-Host 'MusicZero removed. Reopen terminals to refresh PATH.'
    exit 0
}

if (-not $Source) {
    # The script ships next to the extracted archive or inside the repository.
    $candidate = Join-Path $PSScriptRoot 'musiczero'
    if (Test-Path $candidate) {
        $Source = $candidate
    } else {
        $zip = Get-ChildItem $PSScriptRoot -Filter 'musiczero-windows-*.zip' |
            Sort-Object LastWriteTime -Descending | Select-Object -First 1
        if ($zip) {
            $Source = $zip.FullName
        } else {
            throw "No musiczero folder or musiczero-windows-*.zip next to the script.`nPass -Source <path>."
        }
    }
}

if ($Source -like '*.zip') {
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [System.IO.Compression.ZipFile]::OpenRead($Source)
    try {
        $names = $zip.Entries.FullName
        if (-not ($names -contains 'musiczero/mz.exe' -or $names -contains 'musiczero\mz.exe')) {
            throw "Packaging is incomplete: mz.exe is missing inside $Source"
        }
    } finally {
        $zip.Dispose()
    }
    if ($Check) {
        exit 0
    }
    $temp = Join-Path ([System.IO.Path]::GetTempPath()) ("musiczero-install-" + [guid]::NewGuid().ToString('N').Substring(0, 8))
    New-Item -ItemType Directory -Force $temp | Out-Null
    try {
        Expand-ZipArchive $Source $temp
        $inner = Join-Path $temp 'musiczero'
        if (-not (Test-Path $inner)) {
            $inner = $temp
        }
        Assert-Package $inner
        Backup-Previous $InstallDir
        Copy-Item $inner $InstallDir -Recurse
        Write-Host "Installed from $Source"
    } finally {
        Remove-Item $temp -Recurse -Force -ErrorAction SilentlyContinue
    }
} else {
    Assert-Package $Source
    if ($Check) {
        Write-Host "Package $Source looks complete"
        exit 0
    }
    if ($Source -ne (Join-Path $InstallDir 'musiczero')) {
        Backup-Previous $InstallDir
        Copy-Item $Source $InstallDir -Recurse
    }
    Write-Host "Installed from $Source"
}

# Update the user PATH once.
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$entries = $userPath -split ';' | Where-Object { $_ }
if ($entries -notcontains $PathEntry) {
    [Environment]::SetEnvironmentVariable('Path', (($entries + $PathEntry) -join ';'), 'User')
    Write-Host "Added $PathEntry to the user PATH"
}

if ($Shortcut) {
    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut($ShortcutPath)
    $shortcut.TargetPath = (Join-Path $InstallDir 'musiczero\mz.exe')
    $shortcut.Arguments = 'start local'
    $shortcut.WorkingDirectory = Join-Path $InstallDir 'musiczero'
    $shortcut.Description = 'MusicZero local music player'
    $shortcut.Save()
    Write-Host "Created $ShortcutPath"
}

Write-Host ''
Write-Host 'MusicZero is ready. Open a new terminal and run:'
Write-Host '  mz status     # help and running player status'
Write-Host '  mz settings   # interactive settings menu'
Write-Host '  mz start ymz  # launch the Yandex Music provider'
exit 0
