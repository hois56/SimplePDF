[CmdletBinding()]
param([switch]$Package, [string]$InnoCompiler)
$ErrorActionPreference='Stop'
Push-Location -LiteralPath $PSScriptRoot
try {
    if (!(Get-Command cargo -ErrorAction SilentlyContinue)) {
        $portableTools = Join-Path $env:USERPROFILE 'AppData\Local\Temp\simplepdf-rust-1.100-tools'
        if (Test-Path -LiteralPath (Join-Path $portableTools 'sdk\bin\cargo.exe')) {
            $env:PATH = "$portableTools\sdk\bin;$portableTools\mingw64\bin;$env:PATH"
            $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = 'gcc'
            $env:CC = 'gcc'
            $env:AR = 'ar'
        }
    }
    if (!$env:CARGO_HOME -and (Test-Path -LiteralPath 'target\cargo-home\registry')) {
        $env:CARGO_HOME = Join-Path $PSScriptRoot 'target\cargo-home'
    }
    & cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Rust build failed' }
    $binaryDir = Join-Path $PSScriptRoot 'target\release'
    $exe = Join-Path $binaryDir 'SimplePDF.exe'
    if (!(Test-Path -LiteralPath $exe)) { throw "Missing $exe" }
    if (!$Package) { Write-Output $exe; return }

    $report = Join-Path $PSScriptRoot 'target\release-check'
    $check = Start-Process -FilePath $exe -ArgumentList ('--self-test "' + $report + '"') -WindowStyle Hidden -Wait -PassThru
    if ($check.ExitCode -ne 0) { throw "Verification failed: $report" }
    $version = [Diagnostics.FileVersionInfo]::GetVersionInfo($exe).ProductVersion
    $identity = Select-String -LiteralPath installer/LocalPDF.iss -Pattern '^#define AppVersion "([^"]+)"'
    $version = $version.TrimEnd()
    if ($version -ne $identity.Matches[0].Groups[1].Value) { throw "Installer and binary versions differ: $version" }
    if (!$InnoCompiler) { $InnoCompiler = Join-Path $PSScriptRoot 'installer_tools\InnoSetup\ISCC.exe' }
    if (!(Test-Path -LiteralPath $InnoCompiler)) { $InnoCompiler = (Get-Command ISCC.exe -ErrorAction Stop).Source }

    $payload = Join-Path $PSScriptRoot 'release_rust\SimplePDF'
    $src = Join-Path $payload 'source'
    $licenses = Join-Path $payload 'licenses'
    $out = Join-Path $PSScriptRoot 'installers'
    foreach ($p in $payload,$src,$licenses,$out,(Join-Path $src 'installer')) {
        New-Item -ItemType Directory -Path $p -Force | Out-Null
    }
    foreach ($name in 'SimplePDF.exe','pdfium.dll') { Copy-Item -LiteralPath (Join-Path $binaryDir $name) -Destination $payload -Force }
    foreach ($name in '사용방법.txt','설치_사용법.txt','LICENSE') { Copy-Item -LiteralPath $name -Destination $payload -Force }
    foreach ($name in 'Cargo.toml','Cargo.lock','build.rs','build.ps1','office_convert.ps1','icon.ico','icon_pdf_preview.png','사용방법.txt','설치_사용법.txt','LICENSE') {
        Copy-Item -LiteralPath $name -Destination $src -Force
    }
    Copy-Item -LiteralPath 'src' -Destination $src -Recurse -Force
    foreach ($name in 'LocalPDF.iss','clean_reinstall.iss','install_paths.iss') {
        Copy-Item -LiteralPath (Join-Path 'installer' $name) -Destination (Join-Path $src 'installer') -Force
    }
    $pdfNotices = Join-Path $PSScriptRoot 'native\pdfium\BUILD_LICENSES'
    if (!(Test-Path -LiteralPath $pdfNotices)) { $pdfNotices = Join-Path $PSScriptRoot '..\licenses\PDFium' }
    New-Item -ItemType Directory -Force (Join-Path $licenses 'PDFium') | Out-Null
    Get-ChildItem -LiteralPath $pdfNotices -File | Copy-Item -Destination (Join-Path $licenses 'PDFium') -Force
    $rustc = & rustc -vV
    $hostTarget = (($rustc | Select-String '^host: ').ToString() -replace '^host: ','').Trim()
    $json = (& cargo metadata --locked --format-version 1 --filter-platform $hostTarget) -join [Environment]::NewLine
    if ($LASTEXITCODE -ne 0) { throw 'Cannot obtain dependency licenses' }
    $deps = ($json | ConvertFrom-Json).packages | Where-Object source | Sort-Object name,version
    $listing = foreach ($dep in $deps) {
        $crate = Split-Path -Parent $dep.manifest_path
        $target = Join-Path $licenses ($dep.name + '-' + $dep.version)
        $files = @(Get-ChildItem -LiteralPath $crate -File | Where-Object Name -Match '^(LICENSE|LICENCE|COPYING|NOTICE|OFL|UNLICENSE)')
        if ($dep.name -eq 'epaint_default_fonts') {
            $files += @(Get-ChildItem -LiteralPath (Join-Path $crate 'fonts') -File -Recurse | Where-Object { $_.Extension -in '.txt','.md' })
        }
        foreach ($item in $files) {
            $dest = Join-Path $target ($item.FullName.Substring($crate.Length+1))
            New-Item -ItemType Directory -Force (Split-Path -Parent $dest) | Out-Null
            Copy-Item -LiteralPath $item.FullName -Destination $dest -Force
        }
        "$($dep.name) $($dep.version)    $($dep.license)    $($dep.repository)"
    }
    $listing | Set-Content -LiteralPath (Join-Path $licenses 'Rust-dependencies.txt') -Encoding utf8
    & $InnoCompiler /Q "/DPayloadDir=$payload" "/DProjectDir=$PSScriptRoot" "/O$out" (Join-Path $PSScriptRoot 'installer\LocalPDF.iss')
    if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed' }
    $setup = Get-Item -LiteralPath (Join-Path $out "SimplePDF-Setup-$version.exe")
    $archive = Join-Path $PSScriptRoot 'SimplePDF.zip'
    $tempArchive = Join-Path $PSScriptRoot ('target\SimplePDF-' + [Guid]::NewGuid().ToString('N') + '.zip')
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [IO.Compression.ZipFile]::CreateFromDirectory($payload,$tempArchive,[IO.Compression.CompressionLevel]::Optimal,$true)
    Move-Item -LiteralPath $tempArchive -Destination $archive -Force
    $binary = Join-Path $payload 'SimplePDF.exe'
    $hash = (Get-FileHash -LiteralPath $binary).Hash.ToLowerInvariant()
    [ordered]@{executable=$binary;archive=$archive;archive_bytes=(Get-Item $archive).Length;exe_sha256=$hash;version=$version} |
        ConvertTo-Json | Set-Content -LiteralPath 'release_manifest.json' -Encoding utf8
    [ordered]@{installer=$setup.FullName;bytes=$setup.Length;sha256=(Get-FileHash $setup.FullName).Hash.ToLowerInvariant();payload_sha256=$hash;
        version=$version;install_scope='current-user';pdf_progid='SimplePDF.PDF';default_directory='%LOCALAPPDATA%\Programs\SimplePDF';
        install_mode='uninstall-then-install';removes_previous_installation=$true} |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $out 'installer_manifest.json') -Encoding utf8
    Write-Output $setup.FullName
} finally {
    Pop-Location
}
