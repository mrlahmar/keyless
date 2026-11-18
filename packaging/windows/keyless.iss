; Keyless installer. Per-user, no administrator rights.
; Build: iscc /DAppVersion=1.2.3 /DSourceDir=<folder with keyless.exe, LICENSE, THIRD_PARTY_LICENSES.html> keyless.iss

#ifndef AppVersion
  #error Pass /DAppVersion=x.y.z
#endif
#ifndef SourceDir
  #define SourceDir "..\..\target\release"
#endif
#ifndef OutputDir
  #define OutputDir "..\..\dist"
#endif

[Setup]
AppId={{5B0C8E7A-4F1D-4C55-9C7E-3E2A9D1B6F40}
AppName=Keyless
AppVersion={#AppVersion}
AppVerName=Keyless {#AppVersion}
AppPublisher=The Keyless contributors
AppPublisherURL=https://github.com/mrlahmar/keyless
AppSupportURL=https://github.com/mrlahmar/keyless/issues
DefaultDirName={localappdata}\Programs\Keyless
DisableProgramGroupPage=yes
DisableDirPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.17763
OutputDir={#OutputDir}
OutputBaseFilename=Keyless-Setup-{#AppVersion}-x64
SetupIconFile=..\..\assets\keyless.ico
UninstallDisplayIcon={app}\keyless.exe
UninstallDisplayName=Keyless
LicenseFile={#SourceDir}\LICENSE
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes
CloseApplications=no

[Tasks]
Name: "startup"; Description: "Start Keyless in the tray when I sign in"
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Files]
Source: "{#SourceDir}\keyless.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\THIRD_PARTY_LICENSES.html"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
Source: "{#SourceDir}\LicenseRef-Slint-Royalty-free-2.0.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\OFL-*.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Keyless"; Filename: "{app}\keyless.exe"
Name: "{autodesktop}\Keyless"; Filename: "{app}\keyless.exe"; Tasks: desktopicon

[Registry]
; Same entry the app writes from its settings, so either place can turn it off.
; Removed on uninstall by the [Code] section, only if it points to this installation.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Keyless"; ValueData: """{app}\keyless.exe"" --background"; Tasks: startup

[Run]
Filename: "{app}\keyless.exe"; Description: "Open Keyless"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
Type: filesandordirs; Name: "{userappdata}\Keyless"

[Code]
const
  RunKey = 'Software\Microsoft\Windows\CurrentVersion\Run';

// Keyless runs in the tray with no visible window, so close it before files change.
// Ending the process also removes its keyboard hook, so the keyboard is never left paused.
procedure StopKeyless();
var
  ResultCode: Integer;
begin
  Exec(ExpandConstant('{sys}\taskkill.exe'),'/F /IM keyless.exe', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Sleep(300);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  StopKeyless();
  Result := '';
end;

function InitializeUninstall(): Boolean;
begin
  StopKeyless();
  Result := True;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Command: String;
begin
  if (CurUninstallStep = usUninstall) and RegQueryStringValue(HKCU, RunKey, 'Keyless', Command) then
    if Pos(Lowercase(ExpandConstant('{app}\keyless.exe')), Lowercase(Command)) > 0 then
      RegDeleteValue(HKCU, RunKey, 'Keyless');
end;
