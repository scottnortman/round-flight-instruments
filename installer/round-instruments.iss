#ifndef PackageDir
  #error PackageDir must identify the staged installation files
#endif
#ifndef OutputDir
  #error OutputDir must identify the artifact directory
#endif
#ifndef ReleaseVersion
  #error ReleaseVersion must identify the test release
#endif
#ifndef InstallerVersion
  #error InstallerVersion must be the numeric Windows version
#endif
[Setup]
AppId=RoundFlightInstruments
AppName=Round Instruments
AppVersion={#InstallerVersion}
AppVerName=Round Instruments {#ReleaseVersion} MSFS test release
AppPublisher=Round Flight Instruments contributors
DefaultDirName={localappdata}\Programs\Round Instruments
DefaultGroupName=Round Instruments
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir={#OutputDir}
OutputBaseFilename=round-instruments-{#ReleaseVersion}-setup
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
Name: "{group}\MSFS test instructions"; Filename: "{app}\START-HERE.txt"
[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "RoundFlightInstruments"; ValueData: """{app}\instrument-tray.exe"""; Tasks: startup; Flags: uninsdeletevalue
[Run]
Filename: "{app}\instrument-tray.exe"; Description: "Start Round Instruments"; Flags: nowait postinstall skipifsilent
[UninstallRun]
Filename: "{app}\instrument-tray.exe"; Parameters: "--quit"; Flags: runhidden skipifdoesntexist; RunOnceId: "StopTray"
