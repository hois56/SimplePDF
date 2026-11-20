#ifndef PayloadDir
  #error PayloadDir is required
#endif
#ifndef ProjectDir
  #error ProjectDir is required
#endif
#define AppVersion "1.0.1"

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

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := PrepareCleanReinstall;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then begin
    FinishCleanReinstall;
  end;
end;
