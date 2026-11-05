; Built by installer/build.ps1, which passes /DAppVersion=<workspace version>.
; The setup is a window around `fafnirx install`: it places the program, asks for the server, the token and a folder, and leaves
; the rest (PATH, enrolling, the start at login) to the program, which does the same for every other way of installing it.
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
UninstallDisplayName=fafnirx {#AppVersion}

[Tasks]
Name: addpath; Description: "Add fafnirx to the PATH"; Flags: checkedonce

[Files]
Source: "{#ExePath}"; DestDir: "{app}"; Flags: ignoreversion

[UninstallRun]
; Stops the client and the tray, takes the start at login and the PATH entry away. The files are removed by this uninstaller.
Filename: "{app}\fafnirx.exe"; Parameters: "uninstall --keep-binary"; Flags: runhidden; RunOnceId: "Uninstall"

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

procedure BrowseFolder(Sender: TObject);
var
  Dir: String;
begin
  Dir := FolderEdit.Text;
  if BrowseForFolder('Select the folder to sync', Dir, False) then
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
  FolderEdit := AddRow(ScaleY(96), 'Folder to sync (optional: leave empty to add folders later from the tray)',
    ExpandConstant('{param:FOLDER|}'), True, @BrowseFolder);
end;

function ShouldSkipPage(PageID: Integer): Boolean;
begin
  // An upgrade of an already enrolled device keeps its settings.
  Result := (PageID = ConfigPage.ID) and FileExists(ConfigFile) and (ExpandConstant('{param:SERVER|}') = '');
end;

function NextButtonClick(CurPageID: Integer): Boolean;
begin
  Result := True;
  // A silent install without /SERVER only installs the files (and the PATH entry).
  if (CurPageID = ConfigPage.ID) and not (WizardSilent and (Trim(ServerEdit.Text) = '')) then
  begin
    if (Trim(ServerEdit.Text) = '') or (Trim(TokenEdit.Text) = '') then
    begin
      MsgBox('Enter the server address and the device token.', mbError, MB_OK);
      Result := False;
    end
    else if (Trim(FolderEdit.Text) <> '') and not DirExists(FolderEdit.Text) then
    begin
      MsgBox('Select an existing folder to sync, or leave it empty.', mbError, MB_OK);
      Result := False;
    end;
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Code: Integer;
begin
  // The client and the tray are ended so that the program file can be replaced (the program being replaced may be one that
  // cannot do it itself); the install starts them again.
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM fafnirx.exe', '', SW_HIDE, ewWaitUntilTerminated, Code);
  Sleep(500);
  Result := '';
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  Args, Log: String;
  Output: AnsiString;
  Code: Integer;
  Ok: Boolean;
begin
  if CurStep <> ssPostInstall then
    Exit;
  Log := ExpandConstant('{tmp}\fafnirx-setup.log');
  Args := 'install --in-place';
  if not WizardIsTaskSelected('addpath') then
    Args := Args + ' --no-path';
  if Trim(ServerEdit.Text) <> '' then
  begin
    Args := Args + ' --server "' + Trim(ServerEdit.Text) + '"';
    if Trim(FolderEdit.Text) <> '' then
      Args := Args + ' --folder "' + RemoveBackslashUnlessRoot(Trim(FolderEdit.Text)) + '"';
    SetEnvironmentVariable('FAFNIRX_TOKEN', Trim(TokenEdit.Text));
    WizardForm.StatusLabel.Caption := 'Connecting to the server...';
  end;
  Ok := Exec(ExpandConstant('{cmd}'),
    '/c ""' + ExpandConstant('{app}\fafnirx.exe') + '" ' + Args + ' > "' + Log + '" 2>&1"',
    '', SW_HIDE, ewWaitUntilTerminated, Code) and (Code = 0);
  SetEnvironmentVariable('FAFNIRX_TOKEN', '');
  if not Ok then
  begin
    LoadStringFromFile(Log, Output);
    SuppressibleMsgBox('fafnirx was installed but could not be set up:' + #13#10#13#10 + String(Output) + #13#10 +
      'Fix the problem and run "fafnirx install --server ..." again, or re-run this setup.', mbError, MB_OK, IDOK);
  end;
end;
