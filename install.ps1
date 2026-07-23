$ErrorActionPreference = "Stop"

$Repository = "juanandresgs/NearManager"
$Binaries = @("near-fm", "near-view", "near-proc", "near-demo")
$DetectedOS = if ($env:NEAR_INSTALL_OS) { $env:NEAR_INSTALL_OS } else { "windows" }
if ($DetectedOS.ToLowerInvariant() -notin @("windows", "win32nt")) {
    throw "Near Manager install: unsupported operating system: $DetectedOS"
}

$DetectedArch = if ($env:NEAR_INSTALL_ARCH) {
    $env:NEAR_INSTALL_ARCH.ToLowerInvariant()
} else {
    [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString().ToLowerInvariant()
}
if ($DetectedArch -notin @("x64", "x86_64", "amd64")) {
    throw "Near Manager install: Windows releases currently support x86_64; detected $DetectedArch"
}

$Archive = "near-windows-x86_64.zip"
if ($env:NEAR_INSTALL_DRY_RUN -eq "1") {
    Write-Output $Archive
    exit 0
}

$InstallDir = if ($env:NEAR_INSTALL_DIR) {
    $env:NEAR_INSTALL_DIR
} else {
    Join-Path $env:LOCALAPPDATA "NearManager\bin"
}
$BaseUrl = if ($env:NEAR_INSTALL_BASE_URL) {
    $env:NEAR_INSTALL_BASE_URL.TrimEnd("/")
} else {
    "https://github.com/$Repository/releases/latest/download"
}
if (-not $BaseUrl.StartsWith("https://") -and $env:NEAR_INSTALL_ALLOW_INSECURE -ne "1") {
    throw "Near Manager install: release URL must use HTTPS"
}

$Temporary = Join-Path ([IO.Path]::GetTempPath()) ("near-manager-install-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $Temporary | Out-Null
try {
    $ArchivePath = Join-Path $Temporary $Archive
    $ChecksumPath = "$ArchivePath.sha256"
    Write-Host "Installing Near Manager for windows/x86_64..."
    Invoke-WebRequest "$BaseUrl/$Archive" -OutFile $ArchivePath
    Invoke-WebRequest "$BaseUrl/$Archive.sha256" -OutFile $ChecksumPath

    $ExpectedHash = ((Get-Content $ChecksumPath -Raw).Trim() -split "\s+")[0].ToLowerInvariant()
    $ActualHash = (Get-FileHash $ArchivePath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($ExpectedHash -ne $ActualHash) {
        throw "Near Manager install: release checksum verification failed"
    }

    $Extracted = Join-Path $Temporary "extracted"
    Expand-Archive -Path $ArchivePath -DestinationPath $Extracted
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    foreach ($Binary in $Binaries) {
        $Source = Join-Path $Extracted "$Binary.exe"
        if (-not (Test-Path $Source -PathType Leaf)) {
            throw "Near Manager install: release archive is missing $Binary.exe"
        }
        Copy-Item $Source (Join-Path $InstallDir "$Binary.exe") -Force
    }

    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $PathEntries = @($UserPath -split ";" | Where-Object { $_ })
    if (-not ($PathEntries | Where-Object { $_.TrimEnd("\") -ieq $InstallDir.TrimEnd("\") })) {
        $NewUserPath = (@($InstallDir) + $PathEntries) -join ";"
        [Environment]::SetEnvironmentVariable("Path", $NewUserPath, "User")
        Write-Host "Added $InstallDir to the user PATH."
    }

    & (Join-Path $InstallDir "near-fm.exe") --version
    Write-Host "Near Manager is installed. Open a new terminal and run: near-fm"
} finally {
    Remove-Item -Recurse -Force $Temporary -ErrorAction SilentlyContinue
}
