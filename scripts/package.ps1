[CmdletBinding()]
param(
    [switch]$Offline,
    [switch]$SkipChecks,
    [switch]$SkipMsi,
    [string]$WixDir
)

$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location $projectRoot

try {
    Write-Host "[1/6] Running verification checks..."
    if (-not $SkipChecks) {
        $verifyScript = Join-Path $PSScriptRoot 'verify.ps1'
        & $verifyScript -Offline:$Offline -Native -Frontend
        if ($LASTEXITCODE -ne 0) { throw "Verification suite failed." }
    } else {
        Write-Host "Skipping verification checks (-SkipChecks specified)."
    }

    Write-Host "[2/6] Building release binary..."
    $cargoOptions = @('build', '--release', '--locked')
    if ($Offline) { $cargoOptions += '--offline' }
    & cargo @cargoOptions
    if ($LASTEXITCODE -ne 0) {
        throw "Release build failed. If Papyrus is currently running, close it and re-run."
    }

    Write-Host "[3/6] Reading package metadata..."
    $version = $null
    try {
        $metaJson = cargo metadata --no-deps --format-version 1 2>$null
        if ($metaJson) {
            $metadata = $metaJson | ConvertFrom-Json
            if ($metadata.packages -and $metadata.packages.Count -gt 0) {
                $version = $metadata.packages[0].version
            }
        }
    } catch { }

    if (-not $version) {
        $manifest = Get-Content (Join-Path $projectRoot 'Cargo.toml') -Raw
        $match = [regex]::Match($manifest, '(?m)^version\s*=\s*"([^"]+)"')
        if ($match.Success) {
            $version = $match.Groups[1].Value
        }
    }
    if (-not $version) { throw "Cannot read package version from Cargo.toml." }
    Write-Host "Detected package version: $version"

    $distDir = Join-Path $projectRoot 'dist'
    New-Item -ItemType Directory -Force -Path $distDir | Out-Null

    Write-Host "[4/6] Copying release artifacts..."
    $releaseExe = Join-Path $projectRoot 'target\release\papyrus.exe'
    if (-not (Test-Path $releaseExe)) {
        throw "Release executable not found at $releaseExe"
    }

    $distExe = Join-Path $distDir 'Papyrus.exe'
    try {
        Copy-Item -LiteralPath $releaseExe -Destination $distExe -Force
    } catch {
        throw "Failed to copy '$releaseExe' to '$distExe'. If Papyrus is running, please close it. Error: $_"
    }
    Copy-Item -LiteralPath (Join-Path $projectRoot 'README.md') -Destination $distDir -Force

    Write-Host "Creating portable ZIP distribution..."
    $zipName = "Papyrus-v$version-windows-x64.zip"
    $zipPath = Join-Path $distDir $zipName
    $zipCreated = $false
    for ($attempt = 1; $attempt -le 5; $attempt++) {
        try {
            Remove-Item -LiteralPath $zipPath -Force -ErrorAction SilentlyContinue
            Compress-Archive -LiteralPath $distExe, (Join-Path $distDir 'README.md') -DestinationPath $zipPath -Force
            $zipCreated = $true
            break
        } catch {
            Start-Sleep -Milliseconds 300
        }
    }
    if (-not $zipCreated) {
        throw "Failed to create archive '$zipPath'."
    }

    Write-Host "[5/6] Building WiX MSI installer..."
    $msiGenerated = $false
    $msiName = "Papyrus-v$version-x64.msi"
    $msiPath = Join-Path $distDir $msiName

    if (-not $SkipMsi) {
        $wix = $null
        # 1. Custom WixDir if provided
        if ($WixDir -and (Test-Path $WixDir)) {
            $c = Join-Path $WixDir 'candle.exe'
            $l = Join-Path $WixDir 'light.exe'
            if ((Test-Path $c) -and (Test-Path $l)) {
                $wix = @{ Candle = $c; Light = $l }
            }
        }

        # 2. Check PATH
        if (-not $wix) {
            $cCmd = Get-Command 'candle.exe' -ErrorAction SilentlyContinue
            $lCmd = Get-Command 'light.exe' -ErrorAction SilentlyContinue
            if ($cCmd -and $lCmd) {
                $wix = @{ Candle = $cCmd.Source; Light = $lCmd.Source }
            }
        }

        # 3. Known installation locations
        if (-not $wix) {
            $candidates = [System.Collections.Generic.List[string]]::new()
            if ($env:WIX) { $candidates.Add((Join-Path $env:WIX 'bin')) }
            $pf86 = ${env:ProgramFiles(x86)}
            if ($pf86) {
                $candidates.Add((Join-Path $pf86 'WiX Toolset v3.11\bin'))
                $candidates.Add((Join-Path $pf86 'WiX Toolset v3.14\bin'))
            }
            $pf = $env:ProgramFiles
            if ($pf) {
                $candidates.Add((Join-Path $pf 'WiX Toolset v3.11\bin'))
                $candidates.Add((Join-Path $pf 'WiX Toolset v3.14\bin'))
            }
            $candidates.Add((Join-Path $projectRoot '..\wix311'))
            $candidates.Add((Join-Path $projectRoot 'wix311'))
            $candidates.Add('F:\Benali\wix311')

            foreach ($dir in $candidates) {
                if (Test-Path $dir) {
                    $c = Join-Path $dir 'candle.exe'
                    $l = Join-Path $dir 'light.exe'
                    if ((Test-Path $c) -and (Test-Path $l)) {
                        $wix = @{ Candle = $c; Light = $l }
                        break
                    }
                }
            }
        }

        if ($wix) {
            Write-Host "Found WiX Toolset: $($wix.Candle)"
            $wxsPath = Join-Path $projectRoot 'installer\Papyrus.wxs'
            $wixobjPath = Join-Path $projectRoot 'installer\Papyrus.wixobj'

            Write-Host "Compiling WiX source (candle.exe)..."
            $candleArgs = @(
                '-nologo',
                '-arch', 'x64',
                "-dVersion=$version",
                "-dSourceDir=$projectRoot",
                '-out', $wixobjPath,
                $wxsPath
            )
            & $wix.Candle @candleArgs
            if ($LASTEXITCODE -ne 0) { throw "WiX candle failed with exit code $LASTEXITCODE" }

            Write-Host "Linking WiX installer (light.exe)..."
            Remove-Item -LiteralPath $msiPath -Force -ErrorAction SilentlyContinue
            $pdbPath = [System.IO.Path]::ChangeExtension($msiPath, '.wixpdb')
            Remove-Item -LiteralPath $pdbPath -Force -ErrorAction SilentlyContinue

            $lightArgs = @(
                '-nologo',
                '-sval',
                '-spdb',
                '-ext', 'WixUIExtension',
                '-out', $msiPath,
                $wixobjPath
            )
            & $wix.Light @lightArgs
            if ($LASTEXITCODE -ne 0) { throw "WiX light failed with exit code $LASTEXITCODE" }

            # Cleanup intermediate wixobj
            Remove-Item -LiteralPath $wixobjPath -Force -ErrorAction SilentlyContinue
            $msiGenerated = $true
            Write-Host "MSI generated successfully: $msiPath"
        } else {
            Write-Warning "WiX Toolset (candle.exe / light.exe) not found. Skipping MSI generation."
            Write-Warning "To build MSI, install WiX Toolset v3.11 or place it in '$projectRoot\..\wix311'."
        }
    } else {
        Write-Host "Skipping MSI generation (-SkipMsi specified)."
    }

    Write-Host "[6/6] Generating SHA256 checksums..."
    $artifactsToHash = [System.Collections.Generic.List[string]]::new()
    $artifactsToHash.Add('Papyrus.exe')
    $artifactsToHash.Add($zipName)
    if ($msiGenerated -and (Test-Path $msiPath)) {
        $artifactsToHash.Add($msiName)
    }

    $checksumLines = [System.Collections.Generic.List[string]]::new()
    foreach ($art in $artifactsToHash) {
        $filePath = Join-Path $distDir $art
        if (Test-Path $filePath) {
            $hash = (Get-FileHash -LiteralPath $filePath -Algorithm SHA256).Hash.ToLowerInvariant()
            $checksumLines.Add("$hash  $art")
            # Per-file .sha256 backward compatibility
            "$hash  $art" | Set-Content -Encoding ascii -LiteralPath "$filePath.sha256"
        }
    }

    $sumsPath = Join-Path $distDir 'SHA256SUMS.txt'
    [System.IO.File]::WriteAllLines($sumsPath, $checksumLines)

    Write-Host "`n======================================================="
    Write-Host "Packaging Complete! Release artifacts in: $distDir"
    foreach ($art in $artifactsToHash) {
        $p = Join-Path $distDir $art
        $sz = [math]::Round((Get-Item $p).Length / 1MB, 2)
        Write-Host "  - $art ($sz MB)"
    }
    Write-Host "  - SHA256SUMS.txt"
    Write-Host "======================================================="
} finally {
    Pop-Location
}
