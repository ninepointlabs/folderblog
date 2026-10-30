; Per-user installer for the early Windows test build.
; Unsigned: SmartScreen will warn. No administrator account is required.
;
; Build (from the repo root, after `cargo build --release`):
;   ISCC.exe packaging\windows\folderblog.iss
; Optional: /DMyAppVersion=0.2.11 /DExeSource=...\folderblog.exe /DOutDir=...\dist

#ifndef MyAppVersion
  #define MyAppVersion "0.2.11"
#endif
#ifndef ExeSource
  #define ExeSource "..\..\target\release\folderblog.exe"
#endif
#ifndef OutDir
  #define OutDir "..\..\dist"
#endif

[Setup]
AppId={{C4E8A1F2-7B33-4E0D-9A16-5D2F8B7C01A4}
AppName=folderblog
AppVersion={#MyAppVersion}
AppPublisher=Nine Point Labs
AppPublisherURL=https://github.com/ninepointlabs/folderblog
AppSupportURL=https://github.com/ninepointlabs/folderblog
DefaultDirName={localappdata}\Programs\folderblog
DisableProgramGroupPage=yes
DisableDirPage=no
PrivilegesRequired=lowest
OutputDir={#OutDir}
OutputBaseFilename=folderblog-{#MyAppVersion}-x86_64-windows-setup
Compression=lzma2
SolidCompression=yes
ArchitecturesAllowed=x64
ArchitecturesInstallIn64BitMode=x64
ChangesEnvironment=yes
UninstallDisplayName=folderblog
WizardStyle=modern
CloseApplications=no

[Files]
Source: "{#ExeSource}"; DestDir: "{app}"; DestName: "folderblog.exe"; Flags: ignoreversion

[Tasks]
Name: modifypath; Description: "Add folderblog to my PATH (new terminals can run the folderblog command)"; Flags: checkedonce
Name: logontask; Description: "Start folderblog when I sign in (watches the Blogs folder in my user profile)"; Flags: unchecked

[Code]
function EnvPath(): string;
var
  Paths: string;
begin
  if not RegQueryStringValue(HKCU, 'Environment', 'Path', Paths) then
    Paths := '';
  Result := Paths;
end;

procedure EnvSetPath(Paths: string);
begin
  RegWriteExpandStringValue(HKCU, 'Environment', 'Path', Paths);
end;

function EnvHasPath(Dir: string): Boolean;
var
  Paths, Item, U: string;
  P: Integer;
begin
  Result := False;
  Paths := EnvPath();
  U := Uppercase(Dir);
  while Paths <> '' do
  begin
    P := Pos(';', Paths);
    if P = 0 then
    begin
      Item := Paths;
      Paths := '';
    end
    else
    begin
      Item := Copy(Paths, 1, P - 1);
      Paths := Copy(Paths, P + 1, Length(Paths));
    end;
    if Uppercase(Item) = U then
    begin
      Result := True;
      exit;
    end;
  end;
end;

procedure EnvAddPath(Dir: string);
var
  Paths: string;
begin
  if EnvHasPath(Dir) then
    exit;
  Paths := EnvPath();
  if (Paths <> '') and (Paths[Length(Paths)] <> ';') then
    Paths := Paths + ';';
  EnvSetPath(Paths + Dir);
end;

procedure EnvRemovePath(Dir: string);
var
  Paths, Item, NewPaths, U: string;
  P: Integer;
begin
  Paths := EnvPath();
  NewPaths := '';
  U := Uppercase(Dir);
  while Paths <> '' do
  begin
    P := Pos(';', Paths);
    if P = 0 then
    begin
      Item := Paths;
      Paths := '';
    end
    else
    begin
      Item := Copy(Paths, 1, P - 1);
      Paths := Copy(Paths, P + 1, Length(Paths));
    end;
    if (Item <> '') and (Uppercase(Item) <> U) then
    begin
      if NewPaths <> '' then
        NewPaths := NewPaths + ';';
      NewPaths := NewPaths + Item;
    end;
  end;
  EnvSetPath(NewPaths);
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  ResultCode: Integer;
  Exe: string;
begin
  if CurStep = ssPostInstall then
  begin
    if WizardIsTaskSelected('modifypath') then
      EnvAddPath(ExpandConstant('{app}'));
    if WizardIsTaskSelected('logontask') then
    begin
      Exe := ExpandConstant('{app}\folderblog.exe');
      Exec(Exe, 'install-service', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
    end;
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  ResultCode: Integer;
begin
  if CurUninstallStep = usUninstall then
  begin
    Exec(ExpandConstant('{app}\folderblog.exe'), 'install-service --uninstall', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
    EnvRemovePath(ExpandConstant('{app}'));
  end;
end;
