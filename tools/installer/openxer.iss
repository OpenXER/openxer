; OpenXER Setup (Inno Setup 6). Built by `desktop/build_release.py --installer`,
; which passes the version from Cargo and the folder of packaged files:
;
;   ISCC.exe /DAppVersion=0.1.0 /DSourceDir=<package folder> /DOutputDir=<folder>
;            tools\installer\openxer.iss
;
; Optional: /DSignTool=<name> with /S<name>=<command> signs Setup and the
; uninstaller (the build script fills both from --sign-command).
;
; Per-user install by default (no administrator prompt), with the standard
; choice to install for all users. Data never lives in the program folder:
; the marker file `openxer.installed` beside OpenXER.exe tells the app to
; keep its workspace in %LOCALAPPDATA%\OpenXER (or OPENXER_DATA_DIR).

#ifndef AppVersion
  #error Define AppVersion (/DAppVersion=x.y.z); build_release.py reads it from Cargo.toml.
#endif
#ifndef SourceDir
  #error Define SourceDir (/DSourceDir=<folder of packaged files>).
#endif
#ifndef OutputDir
  #define OutputDir "."
#endif
#ifndef VersionNumbers
  #define VersionNumbers AppVersion
#endif

#define AppName "OpenXER"
#define AppExe "OpenXER.exe"
#define CliExe "openxer-cli.exe"
#define ProgId "OpenXER.xer"
#define RepoRoot AddBackslash(SourcePath) + "..\.."

[Setup]
; Never change AppId: upgrades find the earlier install by it.
AppId={{178563E0-F540-4DE9-A01C-58D35FFD3E1E}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=OpenXER
AppPublisherURL=https://github.com/openxer/openxer
AppSupportURL=https://github.com/openxer/openxer/issues
AppUpdatesURL=https://github.com/openxer/openxer/releases
AppCopyright=Copyright (c) 2026 The OpenXER Contributors
VersionInfoVersion={#VersionNumbers}
VersionInfoProductName={#AppName}
VersionInfoProductVersion={#VersionNumbers}
VersionInfoDescription={#AppName} Setup
VersionInfoCompany=OpenXER
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
AllowNoIcons=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog commandline
; 64-bit Windows 10 or later (x64, or Arm64 Windows 11 running x64 programs).
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
DisableWelcomePage=no
DisableDirPage=auto
DisableProgramGroupPage=auto
LicenseFile={#SourcePath}license.txt
SetupIconFile={#RepoRoot}\assets\icons\openxer.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
WizardStyle=modern
ChangesAssociations=yes
; Running copies: the app holds this mutex (desktop/src/instance.rs), so
; Setup and the uninstaller ask the user to close it. Other files in use
; (openxer-cli serving an AI client) are listed on the Preparing page,
; where the user chooses whether Setup closes them.
AppMutex=OpenXER.App,Global\OpenXER.App
CloseApplications=yes
RestartApplications=no
OutputDir={#OutputDir}
OutputBaseFilename={#AppName}-{#AppVersion}-setup
Compression=lzma2/max
SolidCompression=yes
#ifdef SignTool
SignTool={#SignTool}
SignedUninstaller=yes
#endif

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Messages]
UninstalledAll=%1 was removed from your computer.%n%nYour schedules were not removed. The workspace (openxer.db), its backups and crash reports stay in the folder AppData\Local\OpenXER in your user profile (or the folder OPENXER_DATA_DIR names). Delete that folder yourself only if you no longer need them.
UninstalledMost=%1 was removed from your computer, but some elements could not be removed. You can remove them yourself.%n%nYour schedules were not removed. The workspace (openxer.db), its backups and crash reports stay in the folder AppData\Local\OpenXER in your user profile (or the folder OPENXER_DATA_DIR names).

[CustomMessages]
AssocGroup=File types:
AssocTask=Open .xer files with OpenXER when double-clicked
RunningSilent=OpenXER is running from %1 (%2). Close it and run Setup again.
RunningUninstall=OpenXER is running from %1 (%2).%n%nClose it (and any AI client using openxer-cli), then click Retry. Cancel stops the uninstall.

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "associate"; Description: "{cm:AssocTask}"; GroupDescription: "{cm:AssocGroup}"; Flags: unchecked

[Files]
Source: "{#SourceDir}\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\{#CliExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\CLI-MCP.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\LICENSE-MIT"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\LICENSE-APACHE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\THIRD_PARTY.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourcePath}openxer.installed"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExe}"; WorkingDir: "{app}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; WorkingDir: "{app}"; Tasks: desktopicon

[Registry]
; "Open with" registration, always: OpenXER is offered for .xer files and
; nothing else changes. Whether it is the default is the "associate" task
; ([Code] below), so software that already opens .xer files keeps doing so.
Root: HKA; Subkey: "Software\Classes\{#ProgId}"; ValueType: string; ValueName: ""; ValueData: "XER schedule"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\{#ProgId}\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"",0"
Root: HKA; Subkey: "Software\Classes\{#ProgId}\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
; The .xer keys may belong to other software: only this value is removed;
; [Code] removes the keys themselves only when this Setup made them.
Root: HKA; Subkey: "Software\Classes\.xer\OpenWithProgids"; ValueType: string; ValueName: "{#ProgId}"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "{#AppName}"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".xer"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""

[UninstallDelete]
; Written by [Code] with the installed path of openxer-cli.exe.
Type: files; Name: "{app}\openxer.mcp.json"

[Run]
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent runasoriginaluser

[Code]
const
  ClassesKey = 'Software\Classes\';
  PreviousValue = 'OpenXERPreviousDefault';
  MadeKeysValue = 'OpenXERMadeKeys';

var
  { Before the first install: which .xer keys did not exist yet. }
  MadeExtKey, MadeOpenWithKey: Boolean;
  { Read at uninstall, before the ProgID key holding it is removed. }
  MadeKeys: String;

{ The per-user data folder the app uses (desktop/src/util.rs). }
function DataFolder(): String;
begin
  Result := GetEnv('OPENXER_DATA_DIR');
  if Result = '' then
    Result := ExpandConstant('{localappdata}\OpenXER');
end;

{ Programs running from folder Dir, as "name, name" (empty when none). }
function RunningFrom(Dir: String): String;
var
  Locator, Service, Found, Item: Variant;
  I: Integer;
  Path: String;
begin
  Result := '';
  try
    Locator := CreateOleObject('WbemScripting.SWbemLocator');
    Service := Locator.ConnectServer('.', 'root\CIMV2');
    Found := Service.ExecQuery('SELECT Name, ExecutablePath FROM Win32_Process WHERE Name = ''{#AppExe}'' OR Name = ''{#CliExe}''');
    for I := 0 to Found.Count - 1 do
    begin
      Item := Found.ItemIndex(I);
      if not VarIsNull(Item.ExecutablePath) then
      begin
        Path := Item.ExecutablePath;
        if CompareText(RemoveBackslashUnlessRoot(ExtractFileDir(Path)), RemoveBackslashUnlessRoot(Dir)) = 0 then
        begin
          if Result <> '' then
            Result := Result + ', ';
          Result := Result + ExtractFileName(Path);
        end;
      end;
    end;
  except
    Log('Could not list running programs: ' + GetExceptionMessage);
  end;
end;

{ A silent install never closes a running OpenXER or openxer-cli: it stops
  with exit code 7 and says why in the log. An interactive install shows
  the running programs on the Preparing page and the user decides. }
function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Running: String;
begin
  Result := '';
  if WizardSilent then
  begin
    Running := RunningFrom(ExpandConstant('{app}'));
    if Running <> '' then
      Result := FmtMessage(CustomMessage('RunningSilent'), [ExpandConstant('{app}'), Running]);
  end;
end;

function JsonString(S: String): String;
var
  I: Integer;
  C: Char;
begin
  Result := '"';
  for I := 1 to Length(S) do
  begin
    C := S[I];
    if C = '\' then
      Result := Result + '\\'
    else if C = '"' then
      Result := Result + '\"'
    else if (Ord(C) < 32) or (Ord(C) > 126) then
      Result := Result + '\u' + Format('%.4x', [Ord(C)])
    else
      Result := Result + C;
  end;
  Result := Result + '"';
end;

{ The MCP client configuration, with the installed openxer-cli.exe. Plain
  ASCII (other letters are JSON escapes), so it reads the same everywhere. }
procedure WriteMcpConfig();
var
  Text: String;
begin
  Text := '{' + #10 +
    '  "mcpServers": {' + #10 +
    '    "openxer": {' + #10 +
    '      "command": ' + JsonString(ExpandConstant('{app}\{#CliExe}')) + ',' + #10 +
    '      "args": [' + #10 +
    '        "mcp"' + #10 +
    '      ]' + #10 +
    '    }' + #10 +
    '  }' + #10 +
    '}' + #10;
  if not SaveStringToFile(ExpandConstant('{app}\openxer.mcp.json'), Text, False) then
    Log('Could not write openxer.mcp.json');
end;

{ A workspace a portable copy kept in this folder moves to the data folder,
  where an installed copy looks for it. Only a closed, whole workspace is
  moved (no write-ahead file beside it), and never over one already there;
  otherwise it stays and the app keeps using it until the data folder has
  one of its own. }
procedure MoveLegacyWorkspace();
var
  App, Data, Old, New: String;
begin
  App := ExpandConstant('{app}');
  Old := App + '\openxer.db';
  if not FileExists(Old) then
    Exit;
  Data := DataFolder();
  New := Data + '\openxer.db';
  if FileExists(New) then
  begin
    Log('Workspace left in place: ' + Old + ' (' + New + ' already exists)');
    if not WizardSilent then
      MsgBox('A workspace from an earlier copy of OpenXER was found in the program folder:' + #13#10 + Old + #13#10#13#10 +
        'It was left where it is, because OpenXER already has a workspace at:' + #13#10 + New + #13#10#13#10 +
        'To use the older one, export its projects from a portable copy, or replace the newer file with it while OpenXER is closed.',
        mbInformation, MB_OK);
    Exit;
  end;
  if FileExists(Old + '-wal') then
  begin
    Log('Workspace left in place: ' + Old + ' (it has a write-ahead file, so it may be open)');
    Exit;
  end;
  if not ForceDirectories(Data) then
  begin
    Log('Workspace left in place: cannot create ' + Data);
    Exit;
  end;
  if RenameFile(Old, New) then
  begin
    Log('Workspace moved: ' + Old + ' -> ' + New);
    if DirExists(App + '\backups') and not DirExists(Data + '\backups') then
      if RenameFile(App + '\backups', Data + '\backups') then
        Log('Backups moved to ' + Data + '\backups');
    if not WizardSilent then
      MsgBox('Your workspace was moved out of the program folder, so upgrades and uninstalls never touch it. It is now at:' + #13#10 + New,
        mbInformation, MB_OK);
  end
  else
    Log('Workspace left in place: could not move ' + Old);
end;

{ Make OpenXER the default for .xer, keeping the earlier default to put
  back on uninstall. }
procedure TakeDefault();
var
  Current: String;
begin
  if RegQueryStringValue(HKA, ClassesKey + '.xer', '', Current) and (Current <> '') and (Current <> '{#ProgId}') then
    RegWriteStringValue(HKA, ClassesKey + '{#ProgId}', PreviousValue, Current);
  RegWriteStringValue(HKA, ClassesKey + '.xer', '', '{#ProgId}');
end;

{ Stop being the default for .xer, if OpenXER still is: the earlier default
  comes back when its program is still registered, else none is set. A
  default someone else set since is left alone. }
procedure ReleaseDefault();
var
  Current, Previous: String;
begin
  if not (RegQueryStringValue(HKA, ClassesKey + '.xer', '', Current) and (Current = '{#ProgId}')) then
    Exit;
  if RegQueryStringValue(HKA, ClassesKey + '{#ProgId}', PreviousValue, Previous) and (Previous <> '') and RegKeyExists(HKA, ClassesKey + Previous) then
    RegWriteStringValue(HKA, ClassesKey + '.xer', '', Previous)
  else
    RegDeleteValue(HKA, ClassesKey + '.xer', '');
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssInstall then
  begin
    MadeExtKey := not RegKeyExists(HKA, ClassesKey + '.xer');
    MadeOpenWithKey := not RegKeyExists(HKA, ClassesKey + '.xer\OpenWithProgids');
  end;
  if CurStep = ssPostInstall then
  begin
    { Recorded on the first install only: an upgrade finds the keys the
      first install made. }
    if not RegValueExists(HKA, ClassesKey + '{#ProgId}', MadeKeysValue) then
    begin
      MadeKeys := '';
      if MadeExtKey then
        MadeKeys := MadeKeys + 'ext ';
      if MadeOpenWithKey then
        MadeKeys := MadeKeys + 'openwith ';
      RegWriteStringValue(HKA, ClassesKey + '{#ProgId}', MadeKeysValue, MadeKeys);
    end;
    WriteMcpConfig();
    MoveLegacyWorkspace();
    if WizardIsTaskSelected('associate') then
      TakeDefault()
    else
      ReleaseDefault();
  end;
end;

{ The uninstaller also refuses to remove files a running openxer-cli holds;
  interactively it asks, silently it stops (the app itself is AppMutex). }
function InitializeUninstall(): Boolean;
var
  Running: String;
begin
  Result := True;
  Running := RunningFrom(ExpandConstant('{app}'));
  while Running <> '' do
  begin
    if UninstallSilent then
    begin
      Log(FmtMessage(CustomMessage('RunningSilent'), [ExpandConstant('{app}'), Running]));
      Result := False;
      Exit;
    end;
    if MsgBox(FmtMessage(CustomMessage('RunningUninstall'), [ExpandConstant('{app}'), Running]), mbError, MB_RETRYCANCEL) <> IDRETRY then
    begin
      Result := False;
      Exit;
    end;
    Running := RunningFrom(ExpandConstant('{app}'));
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
  begin
    if not RegQueryStringValue(HKA, ClassesKey + '{#ProgId}', MadeKeysValue, MadeKeys) then
      MadeKeys := '';
    ReleaseDefault();
  end;
  if CurUninstallStep = usPostUninstall then
  begin
    if Pos('openwith ', MadeKeys) > 0 then
      RegDeleteKeyIfEmpty(HKA, ClassesKey + '.xer\OpenWithProgids');
    if Pos('ext ', MadeKeys) > 0 then
      RegDeleteKeyIfEmpty(HKA, ClassesKey + '.xer');
    Log('The data folder was not touched: ' + DataFolder());
  end;
end;
