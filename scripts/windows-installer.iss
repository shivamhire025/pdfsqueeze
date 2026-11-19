; Inno Setup script: one pdfsqueeze-windows-x64-setup.exe that installs the staged package.
;
; Built in CI (.github/workflows/release.yml, job windows-x64) after the package is staged and
; smoke-tested:
;   ISCC.exe /DAppVersion=0.1.0 /DSourceDir=C:\path\to\dist\pdfsqueeze-windows-x64\pdfsqueeze ^
;            /OC:\path\to\dist scripts\windows-installer.iss
;
; Per-user install (no administrator rights, no password): files go to
; %LOCALAPPDATA%\Programs\pdfsqueeze, with a Start menu entry and an entry under
; Settings > Apps so it can be uninstalled. Nothing is written outside the user's own folders and
; the user's own uninstall registry key.
;
; Not code-signed, so Windows SmartScreen shows "unknown publisher" once. No PATH change: the
; command line is for scripts and AI agents and has its own installer (install.ps1).

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceDir
  #error SourceDir must point at the staged package folder (the one that contains pdfsqueeze.exe)
#endif

[Setup]
; Keep this GUID forever: it is how Windows recognises an upgrade of the same app.
AppId={{28D0D1F3-10E9-4C0E-B3E3-BB675915C865}
AppName=pdfsqueeze
AppVersion={#AppVersion}
AppVerName=pdfsqueeze {#AppVersion}
AppPublisher=pdfsqueeze (open source, AGPL-3.0)
AppPublisherURL=https://github.com/shivamhire025/pdfsqueeze
AppSupportURL=https://github.com/shivamhire025/pdfsqueeze/issues
AppUpdatesURL=https://github.com/shivamhire025/pdfsqueeze/releases
DefaultDirName={localappdata}\Programs\pdfsqueeze
DefaultGroupName=pdfsqueeze
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputBaseFilename=pdfsqueeze-windows-x64-setup
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\pdfsqueeze.exe
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
; Ask to close a running copy instead of failing on locked files.
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: recursesubdirs createallsubdirs ignoreversion

[Icons]
Name: "{autoprograms}\pdfsqueeze"; Filename: "{app}\pdfsqueeze.exe"
Name: "{autodesktop}\pdfsqueeze"; Filename: "{app}\pdfsqueeze.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\pdfsqueeze.exe"; Description: "Open pdfsqueeze now"; Flags: nowait postinstall skipifsilent
