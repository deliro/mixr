$ErrorActionPreference = "Stop"

$Repo = "deliro/mixr"
$Binary = "mixr"
$Target = "x86_64-pc-windows-msvc"
$InstallDir = if ($env:MIXR_INSTALL_DIR) { $env:MIXR_INSTALL_DIR } else { "$env:USERPROFILE\.local\bin" }

function Get-LatestTag {
    $url = "https://api.github.com/repos/$Repo/releases/latest"
    $release = Invoke-RestMethod -Uri $url -UseBasicParsing
    if (-not $release.tag_name) {
        throw "could not determine latest release"
    }
    return $release.tag_name
}

function Main {
    $tag = Get-LatestTag
    $latest = $tag -replace '^v', ''

    $existingPath = Get-Command $Binary -ErrorAction SilentlyContinue
    if ($existingPath) {
        $versionOutput = & $Binary --version 2>$null
        $current = ($versionOutput -split '\s+')[1]
        if ($current -eq $latest) {
            Write-Host "$Binary $current is already up to date"
            return
        }
        Write-Host "$Binary is already installed (current: $current, latest: $latest)"
        $answer = Read-Host "update? [y/N]"
        if ($answer -notin @("y", "Y", "yes", "Yes")) {
            Write-Host "cancelled"
            return
        }
    }

    $archive = "$Binary-$Target.zip"
    $url = "https://github.com/$Repo/releases/download/$tag/$archive"
    $checksumUrl = "$url.sha256"

    Write-Host "installing $Binary $tag ($Target)"

    $tmpDir = Join-Path ([System.IO.Path]::GetTempPath()) ([System.IO.Path]::GetRandomFileName())
    New-Item -ItemType Directory -Path $tmpDir | Out-Null

    try {
        $archivePath = Join-Path $tmpDir $archive
        $checksumPath = Join-Path $tmpDir "$archive.sha256"

        Invoke-WebRequest -Uri $url -OutFile $archivePath -UseBasicParsing
        Invoke-WebRequest -Uri $checksumUrl -OutFile $checksumPath -UseBasicParsing

        Write-Host -NoNewline "verifying checksum... "
        $expectedLine = (Get-Content $checksumPath -Raw).Trim()
        $expectedHash = ($expectedLine -split '\s+')[0]
        $actualHash = (Get-FileHash $archivePath -Algorithm SHA256).Hash.ToLower()
        if ($actualHash -ne $expectedHash) {
            throw "checksum mismatch: expected $expectedHash, got $actualHash"
        }
        Write-Host "ok"

        Expand-Archive -Path $archivePath -DestinationPath $tmpDir -Force

        if (-not (Test-Path $InstallDir)) {
            New-Item -ItemType Directory -Path $InstallDir | Out-Null
        }

        Copy-Item (Join-Path $tmpDir "$Binary.exe") (Join-Path $InstallDir "$Binary.exe") -Force

        $currentPath = [Environment]::GetEnvironmentVariable("Path", "User")
        if ($currentPath -notlike "*$InstallDir*") {
            [Environment]::SetEnvironmentVariable("Path", "$currentPath;$InstallDir", "User")
            Write-Host "added $InstallDir to user PATH (restart terminal to apply)"
        }

        Write-Host "$Binary installed to $InstallDir\$Binary.exe"
    }
    finally {
        Remove-Item -Recurse -Force $tmpDir -ErrorAction SilentlyContinue
    }
}

Main
