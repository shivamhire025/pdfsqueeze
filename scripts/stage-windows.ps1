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

$gsExeName   = if ($Arch -eq "x86") { "gswin32c.exe" } else { "gswin64c.exe" }

$work = Join-Path ([IO.Path]::GetTempPath()) ("pdfsqueeze-stage-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $work | Out-Null

# Pinned engine releases. Bump these on purpose (and re-check the hashes at
# the release pages) rather than taking whatever "latest" happens to be.
$pins = @{
    "qpdf-x64" = @{ Repo = "qpdf/qpdf"; Tag = "v12.4.2"; Name = "qpdf-12.4.2-msvc64.zip";
                    Sha256 = "db87077e683630c1217e0e8f9a20a9749d952ab676e881c3689187763a5de25d" }
    "qpdf-x86" = @{ Repo = "qpdf/qpdf"; Tag = "v12.4.2"; Name = "qpdf-12.4.2-msvc32.zip";
                    Sha256 = "c4c62fbb5545090e80621a44e5b3063fb828e13b795c24aa637366b0e38ef8c0" }
    "gs-x64"   = @{ Repo = "ArtifexSoftware/ghostpdl-downloads"; Tag = "gs10080"; Name = "gs10080w64.exe";
                    Sha256 = "52a91b8bf09298788d7a57b9206127026c23eacd75405f0a131e26dc381dce50" }
    "gs-x86"   = @{ Repo = "ArtifexSoftware/ghostpdl-downloads"; Tag = "gs10080"; Name = "gs10080w32.exe";
                    Sha256 = "42f9e97ea2580bc24e2604cc03a2213bdba6094869dbe9f8ede50f8dd86b7463" }
}

# Download one pinned asset with the GitHub CLI and refuse it unless its SHA-256 matches.
function Get-Pinned([string]$Key, [string]$Dest) {
    $p = $pins[$Key]
    Write-Host "$($p.Repo) $($p.Tag): $($p.Name)"
    $dir = Join-Path $work ("dl-" + $Key)
    New-Item -ItemType Directory -Path $dir | Out-Null
    gh release download $p.Tag --repo $p.Repo --pattern $p.Name --dir $dir
    if ($LASTEXITCODE -ne 0) { throw "download of $($p.Name) failed" }
    $file = Join-Path $dir $p.Name
    $got = (Get-FileHash $file -Algorithm SHA256).Hash.ToLower()
    if ($got -ne $p.Sha256) { throw "$($p.Name): SHA-256 mismatch (expected $($p.Sha256), got $got)" }
    Move-Item $file $Dest
}

function Expand-7z([string]$Archive, [string]$Dest) {
    $sevenZip = (Get-Command 7z -ErrorAction SilentlyContinue).Source
    if (-not $sevenZip) { $sevenZip = "C:\Program Files\7-Zip\7z.exe" }
    & $sevenZip x $Archive "-o$Dest" -y | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "7z failed on $Archive" }
}

# ---- fetch -----------------------------------------------------------------
Get-Pinned "qpdf-$Arch" (Join-Path $work "qpdf.zip")
Expand-Archive (Join-Path $work "qpdf.zip") -DestinationPath (Join-Path $work "qpdf") -Force

Get-Pinned "gs-$Arch" (Join-Path $work "gs.exe")
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
$gp = $pins["gs-$Arch"]; $qp = $pins["qpdf-$Arch"]
@("ghostscript $($gp.Tag) ($($gp.Name))", "qpdf $($qp.Tag) ($($qp.Name))") |
    Set-Content (Join-Path $Out "engines/MANIFEST.txt")

Copy-Item LICENSE, README.md, AGENTS.md, CREDITS.md, THIRD-PARTY-NOTICES.md, THIRD-PARTY-LICENSES-RUST.txt $Out

@"
pdfsqueeze is licensed under the GNU AGPL v3 (see LICENSE).
Source code: https://github.com/shivamhire025/pdfsqueeze

This package also bundles unmodified official builds of third-party programs:
Ghostscript (AGPL, https://ghostscript.com) and qpdf (Apache 2.0,
https://github.com/qpdf/qpdf), and for 64-bit builds Tesseract and its
dependencies from MSYS2. engines/MANIFEST.txt lists each with its exact version,
engines/licenses/ holds their licence texts, and THIRD-PARTY-NOTICES.md links to
each upstream project. The corresponding source for those exact versions is
available from the upstream projects; if you cannot obtain it, open an issue at
https://github.com/shivamhire025/pdfsqueeze/issues and it will be provided for
at least three years from the date of your download.
"@ | Set-Content (Join-Path $Out "SOURCE.txt")

Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue

Write-Host "staged $Out"
Get-ChildItem $bin | Select-Object -First 30 | ForEach-Object { Write-Host "  $($_.Name)" }
