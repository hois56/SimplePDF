param([Parameter(Mandatory)][string]$Payload, [Parameter(Mandatory)][string]$Stage, [string]$MakeAppx,
      [Parameter(Mandatory)][ValidatePattern('^[0-9A-Fa-f]{40}$')][string]$Certificate)
$ErrorActionPreference = 'Stop'
$compiler = (Get-Command g++.exe -ErrorAction Stop).Source
& $compiler -std=c++17 -nostdinc++ -fno-rtti -fno-exceptions -O2 -s -shared -static -o (Join-Path $Payload 'SimplePDFExplorer.dll') (Join-Path $PSScriptRoot 'command.cpp') -lole32 -lshlwapi -luuid
if ($LASTEXITCODE) { throw 'Explorer command build failed' }
if (!$MakeAppx) {
    $sdk = Join-Path (${env:ProgramFiles(x86)}) 'Windows Kits\10\bin'
    $MakeAppx = Get-ChildItem -Path "$sdk\*\x64\makeappx.exe" -ErrorAction SilentlyContinue |
        Sort-Object { [version]$_.Directory.Parent.Name } -Descending | Select-Object -First 1 -ExpandProperty FullName
    if (!$MakeAppx) { $MakeAppx = (Get-Command makeappx.exe -ErrorAction Stop).Source }
}
New-Item -ItemType Directory -Path $Stage -Force | Out-Null
$signer = Get-Item -LiteralPath "Cert:\CurrentUser\My\$Certificate" -ErrorAction Stop
$manifest = [xml](Get-Content -LiteralPath (Join-Path $PSScriptRoot 'AppxManifest.xml') -Raw)
$manifest.Package.Identity.Publisher = $signer.Subject
$manifest.Save((Join-Path $Stage 'AppxManifest.xml'))
Copy-Item -LiteralPath (Join-Path $PSScriptRoot '..\..\icon_pdf_preview.png') -Destination (Join-Path $Stage 'Logo.png') -Force
& $MakeAppx pack /d $Stage /p (Join-Path $Payload 'SimplePDF.ExplorerMenu.msix') /nv /o
if ($LASTEXITCODE) { throw 'Explorer identity package build failed' }
$signTool = Join-Path (Split-Path -Parent $MakeAppx) 'signtool.exe'
& $signTool sign /fd SHA256 /sha1 $Certificate (Join-Path $Payload 'SimplePDF.ExplorerMenu.msix')
if ($LASTEXITCODE) { throw 'Explorer identity package signing failed' }
