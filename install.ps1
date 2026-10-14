# pdfsqueeze installer for Windows. Nothing else needs to be installed.
#
#   irm https://raw.githubusercontent.com/shivamhire025/pdfsqueeze/main/install.ps1 | iex
#
# Installs to %LOCALAPPDATA%\pdfsqueeze and adds that folder to your user PATH.
# No administrator rights needed; nothing is written outside your profile.
#
# STATUS: written but NOT YET RUN on Windows.
#
# Parameters (when run as a file):
#   -Zip   <path>   install from a local .zip instead of downloading
#   -Dir   <path>   install location (default %LOCALAPPDATA%\pdfsqueeze)
#   -BaseUrl <url>  download from this folder URL instead of the GitHub release
#                   (the zip and its .sha256 must both be there; used by tests)
param(
    [string]$Zip = "",
    [string]$Dir = (Join-Path $env:LOCALAPPDATA "pdfsqueeze"),
    [string]$BaseUrl = ""
)

$ErrorActionPreference = "Stop"
$repo = "shivamhire025/pdfsqueeze"

# 64-bit Windows gets the x64 build, 32-bit gets x86 (which has no OCR).
$arch = if ([Environment]::Is64BitOperatingSystem) { "x64" } else { "x86" }
$asset = "pdfsqueeze-windows-$arch.zip"

$tmp = Join-Path ([IO.Path]::GetTempPath()) ("pdfsqueeze-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    $pkg = Join-Path $tmp "pkg.zip"
    if ($Zip -ne "") {
        Copy-Item $Zip $pkg
    } else {
        if ($BaseUrl -eq "") { $BaseUrl = "https://github.com/$repo/releases/latest/download" }
        $url = "$BaseUrl/$asset"
        Write-Host "Downloading $url"
        try {
            Invoke-WebRequest -Uri $url -OutFile $pkg -UseBasicParsing
        } catch {
            throw "download failed. Is there a release for Windows $arch yet? ($($_.Exception.Message))"
        }
    }

    if ($Zip -eq "") {
        # Every release file has a .sha256 beside it; refuse a download that does not match.
        $sumFile = Join-Path $tmp "pkg.sha256"
        try { Invoke-WebRequest -Uri "$url.sha256" -OutFile $sumFile -UseBasicParsing }
        catch { throw "could not fetch the checksum file ($url.sha256)" }
        $want = ((Get-Content $sumFile -Raw).Trim() -split "\s+")[0].ToLower()
        $got = (Get-FileHash $pkg -Algorithm SHA256).Hash.ToLower()
        if ($want -ne $got) { throw "checksum mismatch: the download is damaged or not the published file" }
        Write-Host "Checksum verified."
    }

    Expand-Archive -Path $pkg -DestinationPath $tmp -Force
    $src = Join-Path $tmp "pdfsqueeze"
    if (-not (Test-Path (Join-Path $src "pdfsqueeze.exe"))) {
        throw "package looks wrong (no pdfsqueeze.exe inside)"
    }

    # Only replace a previous pdfsqueeze install (or an empty folder), never a random folder.
    if (Test-Path $Dir) {
        $isOurs = Test-Path (Join-Path $Dir "pdfsqueeze.exe")
        $isEmpty = -not (Get-ChildItem -Force $Dir | Select-Object -First 1)
        if (-not ($isOurs -or $isEmpty)) {
            throw "$Dir exists and is not a pdfsqueeze install; choose another -Dir or remove it yourself"
        }
        Remove-Item -Recurse -Force $Dir
    }
    New-Item -ItemType Directory -Path (Split-Path $Dir) -Force | Out-Null
    Move-Item $src $Dir

    # Clear the "downloaded from the internet" mark so SmartScreen does not
    # block each bundled engine individually.
    Get-ChildItem -Recurse -File $Dir | Unblock-File -ErrorAction SilentlyContinue

    # Put the folder on the user PATH (not the machine PATH: no admin needed).
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($null -eq $userPath) { $userPath = "" }
    if (($userPath -split ";") -notcontains $Dir) {
        $newPath = if ($userPath -eq "") { $Dir } else { "$userPath;$Dir" }
        [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
        Write-Host "Added $Dir to your PATH. Open a new terminal to use `pdfsqueeze`."
    }

    Write-Host "Installed: $Dir\pdfsqueeze.exe"
    & (Join-Path $Dir "pdfsqueeze.exe") --version
    & (Join-Path $Dir "pdfsqueeze.exe") doctor
} finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
