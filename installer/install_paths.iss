// Shared by the installer and isolated migration fixtures.
var
  PriorPathLoaded: Boolean;
  PriorPath, MovedFromPath, MovedToPath: String;

function CanonicalInstallPath(const Value: String): String;
begin
  Result := RemoveBackslashUnlessRoot(ExpandFileName(Value));
end;

function PriorInstallPath: String;
begin
  if not PriorPathLoaded then begin
    PriorPathLoaded := True;
    if RegQueryStringValue(HKCU64, MigrationUninstallKey, 'InstallLocation', PriorPath) then
      PriorPath := CanonicalInstallPath(PriorPath);
  end;
  Result := PriorPath;
end;

function IsLegacyInstallPath(const Value: String): Boolean;
var Name: String;
begin
  Name := ExtractFileName(Value);
  Result := (CompareText(Name, 'LocalPDF') = 0) or
            (CompareText(Name, '로컬PDF') = 0) or (CompareText(Name, '로컬 PDF') = 0);
end;

function DefaultSimplePDFDir(Param: String): String;
var OldPath: String;
begin
  OldPath := PriorInstallPath;
  if OldPath = '' then
    Result := CanonicalInstallPath(ExpandConstant(MigrationDefaultDirectory))
  else if IsLegacyInstallPath(OldPath) then
    Result := AddBackslash(ExtractFileDir(OldPath)) + 'SimplePDF'
  else
    Result := OldPath;
end;

function ApplicationFileAvailable(const Filename: String): Boolean;
var Stream: TFileStream;
begin
  Result := True;
  if not FileExists(Filename) then exit;
  try
    { Read/write, exclusive: opening a running executable must fail. No write is made. }
    Stream := TFileStream.Create(Filename, $0002 or $0010);
    Stream.Free;
  except
    Result := False;
  end;
end;

procedure UpdateMovedShortcut(const Filename: String);
var Link, Shell: Variant; Target: String;
begin
  if (MovedFromPath = '') or not FileExists(Filename) then exit;
  try
    Shell := CreateOleObject('WScript.Shell');
    Link := Shell.CreateShortcut(Filename);
    Target := Link.TargetPath;
    if (CompareText(Target, AddBackslash(MovedFromPath) + 'SimplePDF.exe') = 0) or
       (CompareText(Target, AddBackslash(MovedFromPath) + 'LocalPDF.exe') = 0) then begin
      Link.TargetPath := AddBackslash(MovedToPath) + 'SimplePDF.exe';
      Link.WorkingDirectory := MovedToPath;
      Link.IconLocation := AddBackslash(MovedToPath) + 'SimplePDF.exe,0';
      Link.Description := 'SimplePDF';
      Link.Save;
      Log('Updated moved SimplePDF shortcut: ' + Filename);
    end;
  except
    Log('Could not update SimplePDF shortcut: ' + Filename);
  end;
end;

procedure UpdateMovedShortcuts;
var Folder: String; Found: TFindRec; Index: Integer;
begin
  for Index := 0 to 1 do begin
    if Index = 0 then Folder := ExpandConstant('{userdesktop}')
    else Folder := ExpandConstant('{userappdata}\Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar');
    if FindFirst(AddBackslash(Folder) + '*.lnk', Found) then begin
      try
        repeat
          if Found.Attributes and FILE_ATTRIBUTE_DIRECTORY = 0 then
            UpdateMovedShortcut(AddBackslash(Folder) + Found.Name);
        until not FindNext(Found);
      finally
        FindClose(Found);
      end;
    end;
  end;
end;
