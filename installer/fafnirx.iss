; Built by installer/build.ps1, which passes /DAppVersion=<workspace version>.
#ifndef AppVersion
  #error AppVersion is not defined; build with installer\build.ps1
#endif
#ifndef ExePath
  #error ExePath is not defined; build with installer\build.ps1
#endif

[Setup]
AppId={{6B1D5C0E-3F57-4C55-9E0A-FA0F17C1A001}
AppName=fafnirx
AppVersion={#AppVersion}
AppVerName=fafnirx {#AppVersion}
AppPublisher=fafnirx
UsePreviousAppDir=yes
VersionInfoVersion={#AppVersion}
VersionInfoProductVersion={#AppVersion}
VersionInfoProductName=fafnirx
VersionInfoDescription=fafnirx client setup
DefaultDirName={autopf}\fafnirx
DisableProgramGroupPage=yes
; The client belongs to the user who installs it: its program, config and state are in their profile, so nothing needs an administrator.
PrivilegesRequired=lowest
OutputDir=..\dist
OutputBaseFilename=fafnirx-setup-{#AppVersion}
Compression=lzma2
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
ChangesEnvironment=yes
UninstallDisplayName=fafnirx {#AppVersion}

[Tasks]
Name: addpath; Description: "Add fafnirx to the PATH"; Flags: checkedonce

[Files]
Source: "{#ExePath}"; DestDir: "{app}"; Flags: ignoreversion

[Registry]
Root: HKCU; Subkey: "Environment"; ValueType: expandsz; ValueName: "Path"; ValueData: "{olddata};{app}"; Tasks: addpath; Check: NeedsPath(ExpandConstant('{app}'))

[UninstallRun]
Filename: "{app}\fafnirx.exe"; Parameters: "uninstall-service"; Flags: runhidden; RunOnceId: "UninstallService"
Filename: "{sys}\taskkill.exe"; Parameters: "/F /IM fafnirx.exe"; Flags: runhidden; RunOnceId: "KillClient"

[UninstallDelete]
Type: filesandordirs; Name: "{app}"

[Code]
var
  ConfigPage: TWizardPage;
  ServerEdit, TokenEdit, FolderEdit: TNewEdit;

function ConfigFile: string;
begin
  Result := ExpandConstant('{localappdata}\fafnirx\config.toml');
end;

function NeedsPath(Dir: string): Boolean;
var
  Paths: string;
begin
  if not RegQueryStringValue(HKCU, 'Environment', 'Path', Paths) then
    Paths := '';
  Result := Pos(';' + Lowercase(Dir) + ';', ';' + Lowercase(Paths) + ';') = 0;
end;

procedure BrowseFolder(Sender: TObject);
var
  Dir: String;
begin
  Dir := FolderEdit.Text;
  if BrowseForFolder('Select the folder to back up', Dir, False) then
    FolderEdit.Text := Dir;
end;

function AddRow(Top: Integer; const Caption, Value: String; Browse: Boolean; OnBrowse: TNotifyEvent): TNewEdit;
var
  L: TNewStaticText;
  B: TNewButton;
  W: Integer;
begin
  L := TNewStaticText.Create(ConfigPage);
  L.Parent := ConfigPage.Surface;
  L.Caption := Caption;
  L.Top := Top;
  Result := TNewEdit.Create(ConfigPage);
  Result.Parent := ConfigPage.Surface;
  Result.Top := Top + ScaleY(16);
  Result.Text := Value;
  W := ConfigPage.SurfaceWidth;
  if Browse then
  begin
    B := TNewButton.Create(ConfigPage);
    B.Parent := ConfigPage.Surface;
    B.Caption := 'Browse...';
    B.Width := ScaleX(80);
    B.Height := Result.Height;
    B.Left := W - B.Width;
    B.Top := Result.Top;
    B.OnClick := OnBrowse;
    Result.Width := W - B.Width - ScaleX(8);
  end
  else
    Result.Width := W;
end;

// The token goes to the client through its environment, not its command line, which other users could read.
function SetEnvironmentVariable(Name, Value: String): Boolean;
  external 'SetEnvironmentVariableW@kernel32.dll stdcall';

procedure InitializeWizard;
begin
  ConfigPage := CreateCustomPage(wpWelcome, 'Connect this device',
    'Enter the details shown when you added the device in the fafnirx web UI.');
  ServerEdit := AddRow(0, 'Server address (for example nas.lan, or https://fafnirx.example.com)', ExpandConstant('{param:SERVER|}'), False, nil);
  TokenEdit := AddRow(ScaleY(48), 'Device token', ExpandConstant('{param:TOKEN|}'), False, nil);
  TokenEdit.PasswordChar := '*';
  FolderEdit := AddRow(ScaleY(96), 'Folder to back up',
    ExpandConstant('{param:FOLDER|{userdocs}}'), True, @BrowseFolder);
end;

function ShouldSkipPage(PageID: Integer): Boolean;
begin
  // An upgrade of an already enrolled device keeps its settings.
  Result := (PageID = ConfigPage.ID) and FileExists(ConfigFile) and (ExpandConstant('{param:SERVER|}') = '');
end;

function NextButtonClick(CurPageID: Integer): Boolean;
begin
  Result := True;
  // A silent install without /SERVER only installs the files.
  if (CurPageID = ConfigPage.ID) and not (WizardSilent and (Trim(ServerEdit.Text) = '')) then
  begin
    if (Trim(ServerEdit.Text) = '') or (Trim(TokenEdit.Text) = '') then
    begin
      MsgBox('Enter the server address and the device token.', mbError, MB_OK);
      Result := False;
    end
    else if not DirExists(FolderEdit.Text) then
    begin
      MsgBox('Select an existing folder to back up.', mbError, MB_OK);
      Result := False;
    end;
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Code: Integer;
begin
  // The client and the tray are ended so that the program file can be replaced; the setup starts them again.
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM fafnirx.exe', '', SW_HIDE, ewWaitUntilTerminated, Code);
  Sleep(500);
  Result := '';
end;

function RunClient(const Args, LogFile: String): Boolean;
var
  Code: Integer;
begin
  Result := Exec(ExpandConstant('{cmd}'),
    '/c ""' + ExpandConstant('{app}\fafnirx.exe') + '" ' + Args + ' > "' + LogFile + '" 2>&1"',
    '', SW_HIDE, ewWaitUntilTerminated, Code) and (Code = 0);
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  Server, Log: String;
  Output: AnsiString;
begin
  if CurStep <> ssPostInstall then
    Exit;
  if Trim(ServerEdit.Text) = '' then
  begin
    // Upgrade of an enrolled device: keep its settings and start the client and the tray again.
    if FileExists(ConfigFile) then
      RunClient('install-service', ExpandConstant('{tmp}\fafnirx-setup.log'));
    Exit;
  end;
  Server := Trim(ServerEdit.Text);
  if Pos('://', Server) = 0 then
  begin
    if Pos(':', Server) = 0 then
      Server := Server + ':7443';
    Server := 'http://' + Server;
  end;
  Log := ExpandConstant('{tmp}\fafnirx-setup.log');
  WizardForm.StatusLabel.Caption := 'Connecting to the server...';
  SetEnvironmentVariable('FAFNIRX_TOKEN', Trim(TokenEdit.Text));
  if not RunClient('enroll --no-service --server "' + Server + '"', Log)
     or not RunClient('add-folder "' + RemoveBackslashUnlessRoot(FolderEdit.Text) + '"', Log)
     or not RunClient('install-service', Log) then
  begin
    SetEnvironmentVariable('FAFNIRX_TOKEN', '');
    LoadStringFromFile(Log, Output);
    SuppressibleMsgBox('fafnirx was installed but could not be set up:' + #13#10#13#10 + String(Output) + #13#10 +
      'Fix the problem and run "fafnirx enroll ..." again, or re-run this setup.', mbError, MB_OK, IDOK);
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Paths, Dir: String;
  P: Integer;
begin
  if CurUninstallStep = usUninstall then
  begin
    // Make sure no client still holds fafnirx.exe open when the files are removed.
    Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM fafnirx.exe', '', SW_HIDE, ewWaitUntilTerminated, P);
    Sleep(1000);
    Exit;
  end;
  if CurUninstallStep <> usPostUninstall then
    Exit;
  Dir := ExpandConstant('{app}');
  if RegQueryStringValue(HKCU, 'Environment', 'Path', Paths) then
  begin
    P := Pos(';' + Lowercase(Dir), ';' + Lowercase(Paths));
    if P > 0 then
    begin
      Delete(Paths, P, Length(Dir) + 1);
      RegWriteExpandStringValue(HKCU, 'Environment', 'Path', Paths);
    end;
  end;
end;
