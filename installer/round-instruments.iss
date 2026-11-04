#ifndef PackageDir
  #error PackageDir must identify the staged installation files
#endif
#ifndef OutputDir
  #error OutputDir must identify the artifact directory
#endif
[Setup]
AppId=RoundFlightInstruments
AppName=Round Instruments
AppVersion=0.2.0.7
AppVerName=Round Instruments 0.2.0-rc.7 prerelease
AppPublisher=Round Flight Instruments contributors
DefaultDirName={localappdata}\Programs\Round Instruments
DefaultGroupName=Round Instruments
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir={#OutputDir}
OutputBaseFilename=round-instruments-0.2.0-rc.7-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
LicenseFile={#PackageDir}\LICENSE
CloseApplications=yes
RestartApplications=no
UninstallDisplayIcon={app}\instrument-tray.exe
[Tasks]
Name: startup; Description: "Start Round Instruments when I sign in"; Flags: checkedonce
[Files]
Source: "{#PackageDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
[Icons]
Name: "{group}\Round Instruments"; Filename: "{app}\instrument-tray.exe"
Name: "{group}\Setup guide"; Filename: "{app}\START-HERE.md"
[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "RoundFlightInstruments"; ValueData: """{app}\instrument-tray.exe"""; Tasks: startup; Flags: uninsdeletevalue
[Run]
Filename: "{app}\instrument-tray.exe"; Description: "Start Round Instruments"; Flags: nowait postinstall skipifsilent
[UninstallRun]
Filename: "{app}\instrument-tray.exe"; Parameters: "--quit"; Flags: runhidden skipifdoesntexist; RunOnceId: "StopTray"
