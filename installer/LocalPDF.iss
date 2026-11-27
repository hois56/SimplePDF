#ifndef PayloadDir
  #error PayloadDir is required
#endif
#ifndef ProjectDir
  #error ProjectDir is required
#endif
#define AppVersion "1.0.5"

[Setup]
AppId={{6F40E14D-6239-463D-9006-D027C4FF2139}
AppName=SimplePDF
AppVersion={#AppVersion}
AppVerName=SimplePDF {#AppVersion}
AppPublisher=SimplePDF
AppCopyright=SimplePDF contributors
DefaultDirName={code:DefaultSimplePDFDir}
DefaultGroupName=SimplePDF
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.19041
UsePreviousAppDir=no
UninstallLogMode=new
DisableWelcomePage=yes
DisableDirPage=auto
DisableProgramGroupPage=yes
DisableReadyPage=yes
WizardStyle=modern
ShowLanguageDialog=no
ChangesAssociations=yes
CloseApplications=yes
RestartApplications=no
UninstallDisplayName=SimplePDF
UninstallDisplayIcon={app}\SimplePDF.exe
SetupIconFile={#ProjectDir}\icon.ico
OutputBaseFilename=SimplePDF-Setup-{#AppVersion}
Compression=lzma2/fast
SolidCompression=yes
VersionInfoVersion={#AppVersion}
VersionInfoProductName=SimplePDF
VersionInfoDescription=SimplePDF 설치 프로그램
VersionInfoCompany=SimplePDF

[Languages]
Name: "korean"; MessagesFile: "compiler:Languages\Korean.isl"

[Tasks]
Name: "desktopicon"; Description: "바탕 화면 바로가기"; Flags: unchecked

[Files]
Source: "{#PayloadDir}\*"; DestDir: "{app}"; Excludes: "logs\*"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{userprograms}\SimplePDF\SimplePDF"; Filename: "{app}\SimplePDF.exe"; WorkingDir: "{app}"
Name: "{userprograms}\SimplePDF\PDF 기본 앱 설정"; Filename: "{win}\explorer.exe"; Parameters: "ms-settings:defaultapps?registeredAppUser=SimplePDF"; IconFilename: "{app}\SimplePDF.exe"
Name: "{userdesktop}\SimplePDF"; Filename: "{app}\SimplePDF.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Registry]
#define OfficeVerb(Ext) "Software\Classes\SystemFileAssociations\." + Ext + "\shell\SimplePDF.ConvertToPDF"
#sub RegisterOfficeVerb
Root: HKCU; Subkey: "{#OfficeVerb(OfficeFormats[OfficeIndex])}"; ValueType: string; ValueData: "PDF로 변환"; Flags: uninsdeletekey
Root: HKCU; Subkey: "{#OfficeVerb(OfficeFormats[OfficeIndex])}"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\SimplePDF.exe"",0"
Root: HKCU; Subkey: "{#OfficeVerb(OfficeFormats[OfficeIndex])}"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Single"
Root: HKCU; Subkey: "{#OfficeVerb(OfficeFormats[OfficeIndex])}\command"; ValueType: string; ValueData: """{app}\SimplePDF.exe"" --to-pdf ""%1"""
#endsub
#dim OfficeFormats[10] {"doc", "docx", "rtf", "odt", "ppt", "pptx", "xls", "xlsx", "xlsm", "csv"}
#define OfficeIndex
#for {OfficeIndex = 0; OfficeIndex < DimOf(OfficeFormats); OfficeIndex++} RegisterOfficeVerb

Root: HKCU; Subkey: "Software\Classes\SimplePDF.PDF"; ValueType: string; ValueData: "PDF 문서"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\SimplePDF.PDF\DefaultIcon"; ValueType: string; ValueData: """{app}\SimplePDF.exe"",0"
Root: HKCU; Subkey: "Software\Classes\SimplePDF.PDF\shell\open\command"; ValueType: string; ValueData: """{app}\SimplePDF.exe"" ""%1"""
Root: HKCU; Subkey: "Software\Classes\SimplePDF.PDF\Application"; ValueType: string; ValueName: "ApplicationName"; ValueData: "SimplePDF"
Root: HKCU; Subkey: "Software\Classes\SimplePDF.PDF\Application"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "PDF 보기 및 편집"
Root: HKCU; Subkey: "Software\Classes\SimplePDF.PDF\Application"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: "{app}\SimplePDF.exe,0"
Root: HKCU; Subkey: "Software\Classes\.pdf\OpenWithProgids"; ValueType: string; ValueName: "SimplePDF.PDF"; ValueData: ""; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\Applications\SimplePDF.exe"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "SimplePDF"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\Applications\SimplePDF.exe\SupportedTypes"; ValueType: string; ValueName: ".pdf"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\SimplePDF.exe\DefaultIcon"; ValueType: string; ValueData: """{app}\SimplePDF.exe"",0"
Root: HKCU; Subkey: "Software\Classes\Applications\SimplePDF.exe\shell\open\command"; ValueType: string; ValueData: """{app}\SimplePDF.exe"" ""%1"""
Root: HKCU; Subkey: "Software\SimplePDF"; Flags: uninsdeletekeyifempty
Root: HKCU; Subkey: "Software\SimplePDF\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "SimplePDF"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\SimplePDF\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "PDF 보기 및 편집"
Root: HKCU; Subkey: "Software\SimplePDF\Capabilities"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: "{app}\SimplePDF.exe,0"
Root: HKCU; Subkey: "Software\SimplePDF\Capabilities\FileAssociations"; ValueType: string; ValueName: ".pdf"; ValueData: "SimplePDF.PDF"
Root: HKCU; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "SimplePDF"; ValueData: "Software\SimplePDF\Capabilities"; Flags: uninsdeletevalue

; Keep previous Windows UserChoice ProgIDs working after the executable rename.
Root: HKCU; Subkey: "Software\Classes\LocalPDF.PDF"; ValueType: string; ValueData: "PDF 문서"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\LocalPDF.PDF\DefaultIcon"; ValueType: string; ValueData: """{app}\SimplePDF.exe"",0"
Root: HKCU; Subkey: "Software\Classes\LocalPDF.PDF\shell\open\command"; ValueType: string; ValueData: """{app}\SimplePDF.exe"" ""%1"""
Root: HKCU; Subkey: "Software\Classes\LocalPDF.PDF\Application"; ValueType: string; ValueName: "ApplicationName"; ValueData: "SimplePDF"
Root: HKCU; Subkey: "Software\Classes\LocalPDF.PDF\Application"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: "{app}\SimplePDF.exe,0"
Root: HKCU; Subkey: "Software\Classes\Applications\LocalPDF.exe"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "SimplePDF"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\Applications\LocalPDF.exe"; ValueType: string; ValueName: "NoOpenWith"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\LocalPDF.exe\shell\open\command"; ValueType: string; ValueData: """{app}\SimplePDF.exe"" ""%1"""
Root: HKCU; Subkey: "Software\Classes\Applications\LocalPDF.exe\DefaultIcon"; ValueType: string; ValueData: """{app}\SimplePDF.exe"",0"
Root: HKCU; Subkey: "Software\Classes\Applications\LocalPDF.exe\SupportedTypes"; Flags: deletekey
Root: HKCU; Subkey: "Software\Classes\.pdf\OpenWithProgids"; ValueName: "LocalPDF.PDF"; Flags: deletevalue
Root: HKCU; Subkey: "Software\RegisteredApplications"; ValueName: "LocalPDF"; Flags: deletevalue
Root: HKCU; Subkey: "Software\LocalPDF\Capabilities"; Flags: deletekey
Root: HKCU; Subkey: "Software\LocalPDF"; Flags: uninsdeletekeyifempty

[InstallDelete]
Type: files; Name: "{app}\LocalPDF.exe"
Type: files; Name: "{userprograms}\로컬 PDF\로컬 PDF.lnk"
Type: files; Name: "{userprograms}\로컬 PDF\PDF 기본 앱 설정.lnk"
Type: dirifempty; Name: "{userprograms}\로컬 PDF"

[Run]
Filename: "ms-settings:defaultapps?registeredAppUser=SimplePDF"; Description: "PDF 기본 앱 설정 열기"; Flags: shellexec postinstall skipifsilent
Filename: "{app}\SimplePDF.exe"; Description: "SimplePDF 실행"; Flags: nowait postinstall skipifsilent unchecked

[UninstallDelete]
Type: files; Name: "{app}\logs\startup.log"
Type: files; Name: "{app}\logs\previous-uninstall.log"
Type: dirifempty; Name: "{app}\logs"

[Code]
const
  MigrationUninstallKey = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\{6F40E14D-6239-463D-9006-D027C4FF2139}_is1';
  MigrationDefaultDirectory = '{localappdata}\Programs\SimplePDF';

#include "clean_reinstall.iss"

procedure ConfigureExplorerMenu(Uninstalling: Boolean);
var Params: String; ExitCode: Integer; Version: TWindowsVersion;
begin
  GetWindowsVersionEx(Version);
  if Version.Build < 22000 then exit;
  if Uninstalling then
    Params := '$publisher = (Get-AuthenticodeSignature -LiteralPath .\SimplePDF.ExplorerMenu.msix).SignerCertificate.Subject; ' +
              'Get-AppxPackage -Name SimplePDF.ExplorerMenu | Where-Object Publisher -eq $publisher | Remove-AppxPackage'
  else
    Params := 'Add-AppxPackage -Path .\SimplePDF.ExplorerMenu.msix -ExternalLocation (Get-Location).Path';
  Params := '-NoProfile -NonInteractive -Command "$ErrorActionPreference = ''Stop''; ' + Params + '"';
  if (not Exec(ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe'), Params,
               ExpandConstant('{app}'), SW_HIDE, ewWaitUntilTerminated, ExitCode)) or (ExitCode <> 0) then begin
    Log(Format('SimplePDF Explorer package operation failed (%d)', [ExitCode]));
    if not Uninstalling then
      SuppressibleMsgBox('Windows 11 탐색기 첫 메뉴에 PDF 변환을 등록하지 못했습니다. 패키지 서명 신뢰 여부와 Windows 앱 설치 정책을 확인하세요.', mbInformation, MB_OK, IDOK);
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then ConfigureExplorerMenu(True);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := PrepareCleanReinstall;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then begin
    FinishCleanReinstall;
    ConfigureExplorerMenu(False);
  end;
end;
