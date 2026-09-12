<#
    install-native.ps1 - build Musika and put it somewhere Windows can pin it.

    The release binary lives under native/target/, which is gitignored and gets
    wiped by `cargo clean`. A taskbar pin that points there breaks the first
    time you clean the build, so this copies the exe somewhere stable and pins
    that instead.

        powershell -ExecutionPolicy Bypass -File tools\install-native.ps1

    Re-run it after changing the native code to push a new build to the pinned
    copy; the shortcut keeps working because the path never moves.
#>

$ErrorActionPreference = 'Stop'

$repo = Split-Path $PSScriptRoot -Parent
$dest = Join-Path $env:LOCALAPPDATA 'Musika'

# cargo is not always on PATH for a non-login shell.
$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
if (Test-Path $cargoBin) { $env:PATH = "$cargoBin;$env:PATH" }

Write-Host "building release binary..."
Push-Location (Join-Path $repo 'native')
try {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
} finally {
    Pop-Location
}

$exe = Join-Path $repo 'native\target\release\musika.exe'
$ico = Join-Path $repo 'icons\musika.ico'
if (-not (Test-Path $exe)) { throw "no binary at $exe" }
if (-not (Test-Path $ico)) { throw "no icon at $ico - run: python tools\make-icons.py" }

New-Item -ItemType Directory -Force -Path $dest | Out-Null

# A running copy holds a lock on its own exe.
Get-Process -Name musika -ErrorAction SilentlyContinue | ForEach-Object {
    Write-Host "closing the running Musika first..."
    $_.Kill()
    $_.WaitForExit(5000) | Out-Null
}

Copy-Item $exe $dest -Force
Copy-Item $ico $dest -Force
Write-Host "installed to $dest"

$programs = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'

# Clear out the previous name, so the Start Menu doesn't offer both.
Get-Process -Name heptad -ErrorAction SilentlyContinue | ForEach-Object { $_.Kill() }
$oldLnk = Join-Path $programs 'Heptad.lnk'
$oldDir = Join-Path $env:LOCALAPPDATA 'Heptad'
if (Test-Path $oldLnk) {
    Remove-Item $oldLnk -Force
    Write-Host "removed the old Heptad shortcut"
}
if (Test-Path $oldDir) {
    Remove-Item $oldDir -Recurse -Force
    Write-Host "removed the old Heptad install"
}

# The Start Menu is what you actually pin from, so the shortcut goes there.
$lnk = Join-Path $programs 'Musika.lnk'
$shell = New-Object -ComObject WScript.Shell
$sc = $shell.CreateShortcut($lnk)
$sc.TargetPath = Join-Path $dest 'musika.exe'
$sc.WorkingDirectory = $dest
$sc.IconLocation = Join-Path $dest 'musika.ico'
$sc.Description = 'Musika - a seven-chord organ'
$sc.Save()
Write-Host "shortcut  $lnk"

# Repair a taskbar pin left pointing at the old name.
#
# Renaming the app deleted %LOCALAPPDATA%\Heptad, which leaves any existing
# taskbar pin aimed at a binary that no longer exists - a dead icon. The pin is
# a .lnk in the User Pinned folder, and which .lnk owns which taskbar slot is
# recorded in an undocumented binary blob under HKCU\...\Taskband that also
# holds every other pin you have. Renaming or removing the file would mean
# rewriting that blob, and getting it wrong takes out the entire taskbar. So the
# existing shortcut is repointed in place instead: same filename, new target and
# icon. Only a pin that actually points at the old, now-missing path is touched.
$pinDir = Join-Path $env:APPDATA 'Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar'
$stalePin = Join-Path $pinDir 'Heptad.lnk'
$repaired = $false
if (Test-Path $stalePin) {
    $pin = $shell.CreateShortcut($stalePin)
    if ($pin.TargetPath -like '*Heptad*' -and -not (Test-Path $pin.TargetPath)) {
        $pin.TargetPath = Join-Path $dest 'musika.exe'
        $pin.WorkingDirectory = $dest
        $pin.IconLocation = Join-Path $dest 'musika.ico'
        $pin.Description = 'Musika - a seven-chord organ'
        $pin.Save()
        Write-Host "repaired the taskbar pin (it pointed at the deleted Heptad build)"
        $repaired = $true
    }
}

# Windows 10 deliberately removed the "Pin to taskbar" verb from the shell
# automation API - Microsoft treats the taskbar as the user's, not an
# installer's. Try it anyway, since it still exists on some builds and in some
# locales, and say plainly what happened rather than claiming success.
$pinned = $false
try {
    $shellApp = New-Object -ComObject Shell.Application
    $item = $shellApp.Namespace($programs).ParseName('Musika.lnk')

    # Match the PIN verb only. A loose match on "taskbar" also catches
    # "Unpin from taskbar", and when the app is already pinned that is the only
    # taskbar verb Windows offers - so a loose match does not fail to pin, it
    # silently UNPINS. This script did exactly that once. Anchoring on "Pin to"
    # cannot match "Unpin from"; the -notlike is belt and braces.
    $verb = $item.Verbs() | Where-Object {
        $n = $_.Name -replace '&', ''
        $n -like 'Pin to taskbar*' -and $n -notlike 'Unpin*'
    }
    if ($verb) { $verb.DoIt(); $pinned = $true }
} catch {
    $pinned = $false
}

Write-Host ""
if ($repaired) {
    Write-Host "Your existing taskbar pin now launches Musika. It may keep showing"
    Write-Host "the old icon until Explorer refreshes its cache - unpin and re-pin"
    Write-Host "from Start if that bothers you."
} elseif ($pinned) {
    Write-Host "pinned to the taskbar."
} else {
    Write-Host "Windows 10 does not allow pinning programmatically, so the last"
    Write-Host "step is yours: press Start, type 'Musika', right-click it and"
    Write-Host "choose 'Pin to taskbar'."
}
