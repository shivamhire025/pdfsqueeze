# Stage a self-contained Windows package: pdfsqueeze.exe plus Ghostscript and qpdf
# taken from their official releases and unpacked (never run an installer on a
# build machine: the silent Ghostscript installer once hung a job for an hour).
#
#   scripts/stage-windows.ps1 -Arch x64     # then add Tesseract, see below
#   scripts/stage-windows.ps1 -Arch x86     # 32-bit: no OCR
#
# Result: dist/pdfsqueeze-windows-<arch>/pdfsqueeze/
#   pdfsqueeze.exe
#   engines/bin/       Ghostscript + qpdf (and their DLLs)
#   engines/share/ghostscript/{Resource,lib,iccprofiles,fonts}
#
# Tesseract is added afterwards for x64 by scripts/bundle-tesseract-windows.sh
# (run in an MSYS2 MINGW64 shell). It goes in its own engines/tesseract/ folder
# so its DLLs cannot clash with qpdf's.
#
# Needs: gh (authenticated; GH_TOKEN is enough), 7z on PATH, and a built
# pdfsqueeze.exe (cargo build --release [--target i686-pc-windows-msvc]).
#
# STATUS: the x86 flow ran in CI and passed 44 of 45 smoke checks; x64 differs
# only in the file name patterns below.
param(
    [ValidateSet("x64", "x86")][string]$Arch = "x64",
    [string]$Exe = "",
    [string]$Out = ""
)

$ErrorActionPreference = "Stop"

$name = "pdfsqueeze-windows-$Arch"
if ($Out -eq "") { $Out = "dist/$name/pdfsqueeze" }
if ($Exe -eq "") {
    $Exe = if ($Arch -eq "x86") { "target/i686-pc-windows-msvc/release/pdfsqueeze.exe" } else { "target/release/pdfsqueeze.exe" }
}
if (-not (Test-Path $Exe)) { throw "$Exe not found; run cargo build --release first" }

$qpdfPattern = if ($Arch -eq "x86") { "msvc32.*\.zip$" } else { "msvc64.*\.zip$" }
$gsPattern   = if ($Arch -eq "x86") { "w32\.exe$" } else { "w64\.exe$" }
$gsExeName   = if ($Arch -eq "x86") { "gswin32c.exe" } else { "gswin64c.exe" }

$work = Join-Path ([IO.Path]::GetTempPath()) ("pdfsqueeze-stage-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $work | Out-Null

function Get-Asset([string]$Repo, [string]$Pattern) {
    $rel = gh api "repos/$Repo/releases/latest" | ConvertFrom-Json
    $asset = $rel.assets | Where-Object { $_.name -match $Pattern } | Select-Object -First 1
    if (-not $asset) {
        $rel.assets | ForEach-Object { Write-Host "  available: $($_.name)" }
        throw "no asset matching '$Pattern' in the latest $Repo release ($($rel.tag_name))"
    }
    return [pscustomobject]@{ Url = $asset.browser_download_url; Name = $asset.name; Tag = $rel.tag_name }
}

function Expand-7z([string]$Archive, [string]$Dest) {
    $sevenZip = (Get-Command 7z -ErrorAction SilentlyContinue).Source
    if (-not $sevenZip) { $sevenZip = "C:\Program Files\7-Zip\7z.exe" }
    & $sevenZip x $Archive "-o$Dest" -y | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "7z failed on $Archive" }
}

# ---- fetch -----------------------------------------------------------------
$q = Get-Asset "qpdf/qpdf" $qpdfPattern
Write-Host "qpdf: $($q.Name) ($($q.Tag))"
Invoke-WebRequest $q.Url -OutFile (Join-Path $work "qpdf.zip") -UseBasicParsing
Expand-Archive (Join-Path $work "qpdf.zip") -DestinationPath (Join-Path $work "qpdf") -Force

$g = Get-Asset "ArtifexSoftware/ghostpdl-downloads" $gsPattern
Write-Host "Ghostscript: $($g.Name) ($($g.Tag))"
Invoke-WebRequest $g.Url -OutFile (Join-Path $work "gs.exe") -UseBasicParsing
Expand-7z (Join-Path $work "gs.exe") (Join-Path $work "gs")

# ---- locate ----------------------------------------------------------------
$gsExe = Get-ChildItem (Join-Path $work "gs") -Recurse -Filter $gsExeName | Select-Object -First 1
if (-not $gsExe) { throw "$gsExeName not found after unpacking the Ghostscript installer" }
$gsBin = $gsExe.Directory.FullName
$gsRoot = $gsExe.Directory.Parent.FullName

$qpdfExe = Get-ChildItem (Join-Path $work "qpdf") -Recurse -Filter "qpdf.exe" | Select-Object -First 1
if (-not $qpdfExe) { throw "qpdf.exe not found in the qpdf archive" }

# ---- stage -----------------------------------------------------------------
if (Test-Path $Out) { Remove-Item -Recurse -Force $Out }
$bin = Join-Path $Out "engines/bin"
$share = Join-Path $Out "engines/share/ghostscript"
$lic = Join-Path $Out "engines/licenses"
New-Item -ItemType Directory -Force -Path $bin, $share, $lic | Out-Null

Copy-Item $Exe (Join-Path $Out "pdfsqueeze.exe")
Copy-Item (Join-Path $gsBin "*") $bin -Recurse
Copy-Item (Join-Path $qpdfExe.Directory.FullName "*") $bin -Recurse -Force

foreach ($d in "Resource", "lib", "iccprofiles", "fonts") {
    $src = Join-Path $gsRoot $d
    if (Test-Path $src) { Copy-Item $src (Join-Path $share $d) -Recurse }
}

# licences and manifest
Get-ChildItem $gsRoot -Recurse -File | Where-Object { $_.Name -match "^(COPYING|LICENSE|LICENCE)" } |
    Select-Object -First 3 | ForEach-Object { Copy-Item $_.FullName (Join-Path $lic ("ghostscript-" + $_.Name)) }
Get-ChildItem (Join-Path $work "qpdf") -Recurse -File | Where-Object { $_.Name -match "^(LICENSE|NOTICE|COPYING)" } |
    Select-Object -First 3 | ForEach-Object { Copy-Item $_.FullName (Join-Path $lic ("qpdf-" + $_.Name)) }
@("ghostscript $($g.Tag) ($($g.Name))", "qpdf $($q.Tag) ($($q.Name))") |
    Set-Content (Join-Path $Out "engines/MANIFEST.txt")

Copy-Item LICENSE, README.md, AGENTS.md $Out

@"
pdfsqueeze is licensed under the GNU AGPL v3 (see LICENSE).
Source code: https://github.com/shivamhire025/pdfsqueeze

This package also bundles unmodified official builds of third-party programs:
Ghostscript (AGPL, https://ghostscript.com) and qpdf (Apache 2.0,
https://github.com/qpdf/qpdf), and for 64-bit builds Tesseract and its
dependencies from MSYS2. engines/MANIFEST.txt lists each with its version and
engines/licenses/ holds their licence texts.
"@ | Set-Content (Join-Path $Out "SOURCE.txt")

Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue

Write-Host "staged $Out"
Get-ChildItem $bin | Select-Object -First 30 | ForEach-Object { Write-Host "  $($_.Name)" }
