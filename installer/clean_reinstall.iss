#include "install_paths.iss"

var
  PreviousVersionRemoved, ReinstallPathsPrepared: Boolean;
  PreviousUninstallLog: String;

function ValidUninstallerName(const Filename: String): Boolean;
var Stem: String; Index: Integer;
begin
  Stem := ChangeFileExt(ExtractFileName(Filename), '');
  Result := (CompareText(ExtractFileExt(Filename), '.exe') = 0) and
    (CompareText(Copy(Stem, 1, 5), 'unins') = 0) and (Length(Stem) >= 8);
  if not Result then exit;
  for Index := 6 to Length(Stem) do
    if (Stem[Index] < '0') or (Stem[Index] > '9') then begin
      Result := False;
      exit;
    end;
end;

function PrepareCleanReinstall: String;
var
  OldPath, NewPath, ExpectedPath, Command, Uninstaller, DataFile, SourceExe: String;
  ExitCode, Index: Integer;
begin
  Result := '';
  if ReinstallPathsPrepared then exit;
  OldPath := PriorInstallPath;
  if OldPath = '' then exit;
  NewPath := CanonicalInstallPath(ExpandConstant('{app}'));
  if IsLegacyInstallPath(OldPath) then
    ExpectedPath := CanonicalInstallPath(AddBackslash(ExtractFileDir(OldPath)) + 'SimplePDF')
  else ExpectedPath := OldPath;
  if CompareText(NewPath, ExpectedPath) <> 0 then begin
    Result := '재설치 폴더를 ' + ExpectedPath + '로 지정해 주세요.';
    exit;
  end;
  if not PreviousVersionRemoved then begin
    SourceExe := CanonicalInstallPath(ExpandConstant('{srcexe}'));
    if Pos(Lowercase(AddBackslash(OldPath)), Lowercase(SourceExe)) = 1 then begin
      Result := '설치 파일을 기존 프로그램 폴더 밖으로 옮긴 뒤 다시 실행해 주세요.';
      exit;
    end;
    if not ApplicationFileAvailable(AddBackslash(OldPath) + 'SimplePDF.exe') or
       not ApplicationFileAvailable(AddBackslash(OldPath) + 'LocalPDF.exe') then begin
      Result := '열려 있는 SimplePDF를 저장하고 닫은 뒤 다시 설치해 주세요.';
      exit;
    end;
    if (CompareText(OldPath, NewPath) <> 0) and DirExists(NewPath) and not RemoveDir(NewPath) then begin
      Result := '대상 폴더에 기존 파일이 있어 재설치할 수 없습니다: ' + NewPath;
      exit;
    end;
    if not RegQueryStringValue(HKCU64, MigrationUninstallKey, 'UninstallString', Command) then begin
      Result := '기존 설치의 제거 정보를 찾을 수 없습니다.';
      exit;
    end;
    Uninstaller := CanonicalInstallPath(RemoveQuotes(Trim(Command)));
    DataFile := ChangeFileExt(Uninstaller, '.dat');
    if (CompareText(ExtractFileDir(Uninstaller), OldPath) <> 0) or
       not ValidUninstallerName(Uninstaller) or not FileExists(Uninstaller) or not FileExists(DataFile) then begin
      Result := '기존 SimplePDF 제거 프로그램을 확인할 수 없습니다.';
      exit;
    end;
    PreviousUninstallLog := ExpandConstant('{tmp}\SimplePDF-previous-uninstall.log');
    Log('Removing previous SimplePDF installation: ' + OldPath);
    WizardForm.PreparingLabel.Caption := '기존 SimplePDF를 제거하고 새로 설치할 준비를 하는 중...';
    if not Exec(Uninstaller, '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /LOG="' +
                PreviousUninstallLog + '"', ExpandConstant('{tmp}'), SW_HIDE,
                ewWaitUntilTerminated, ExitCode) then begin
      Result := '기존 SimplePDF 제거 프로그램을 실행할 수 없습니다.';
      exit;
    end;
    if ExitCode <> 0 then begin
      Result := '기존 SimplePDF 제거가 완료되지 않아 새 설치를 중단했습니다.';
      exit;
    end;
    for Index := 1 to 100 do begin
      if not FileExists(Uninstaller) and not FileExists(DataFile) then break;
      Sleep(100);
    end;
    if RegKeyExists(HKCU64, MigrationUninstallKey) or FileExists(Uninstaller) or
       FileExists(DataFile) or FileExists(AddBackslash(OldPath) + 'SimplePDF.exe') or
       FileExists(AddBackslash(OldPath) + 'LocalPDF.exe') or
       FileExists(AddBackslash(OldPath) + '_internal\python312.dll') or
       FileExists(AddBackslash(OldPath) + '_internal\PySide6\Qt6Core.dll') then begin
      Result := '기존 프로그램 파일을 완전히 제거하지 못했습니다. 프로그램을 닫은 뒤 다시 시도해 주세요.';
      exit;
    end;
    PreviousVersionRemoved := True;
    Log('Previous SimplePDF installation removed before installing new files.');
  end;
  { Uninstall deletes tracked program files only. Keep any separately saved files. }
  if (CompareText(OldPath, NewPath) <> 0) and DirExists(OldPath) then begin
    if not RenameFile(OldPath, NewPath) then begin
      Result := '보관 파일을 SimplePDF 폴더로 옮길 수 없습니다. 폴더를 사용 중인 프로그램을 닫고 다시 시도해 주세요.';
      exit;
    end;
  end;
  MovedFromPath := OldPath;
  MovedToPath := NewPath;
  ReinstallPathsPrepared := True;
end;

procedure FinishCleanReinstall;
var Directory: String;
begin
  UpdateMovedShortcuts;
  if PreviousVersionRemoved and FileExists(PreviousUninstallLog) then begin
    Directory := ExpandConstant('{app}\logs');
    if ForceDirectories(Directory) then
      FileCopy(PreviousUninstallLog, AddBackslash(Directory) + 'previous-uninstall.log', False);
  end;
end;
