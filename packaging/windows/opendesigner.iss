; Inno Setup script for the OpenDesigner installer. Built by scripts/package-windows.sh:
;   ISCC /DAppVersion=0.0.1 /DSourceDir=<dist\OpenDesigner> /DOutputDir=<dist> [/DIconFile=<app.ico>] opendesigner.iss
; The installer is per user (no administrator prompt) and installs over an earlier version of itself,
; which is how an update is applied: download the new installer from the release page and run it.

#ifndef AppVersion
  #error AppVersion is not set
#endif
#ifndef SourceDir
  #error SourceDir is not set
#endif
#ifndef OutputDir
  #define OutputDir "."
#endif

[Setup]
; Never change the AppId: it is how a new installer finds the copy to replace and how Windows
; lists the uninstaller.
AppId={{6F0B7A52-5C3E-4C2B-9D55-0E7C2A1B8F31}
AppName=OpenDesigner
AppVersion={#AppVersion}
AppPublisher=OpenDesigner contributors
AppPublisherURL=https://github.com/skepjandi/opendesigner
DefaultDirName={autopf}\OpenDesigner
DefaultGroupName=OpenDesigner
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#OutputDir}
OutputBaseFilename=OpenDesigner-{#AppVersion}-windows-x64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
LicenseFile={#SourceDir}\LICENSE
UninstallDisplayIcon={app}\opendesigner-app.exe
CloseApplications=yes
#ifdef IconFile
SetupIconFile={#IconFile}
#endif

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: recursesubdirs createallsubdirs ignoreversion

[Icons]
Name: "{group}\OpenDesigner"; Filename: "{app}\opendesigner-app.exe"
Name: "{autodesktop}\OpenDesigner"; Filename: "{app}\opendesigner-app.exe"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Run]
Filename: "{app}\opendesigner-app.exe"; Description: "Start OpenDesigner"; Flags: nowait postinstall skipifsilent
