#define MyAppName "BigWave Radio"
#define MyAppVersion "0.7.1"
#define MyAppPublisher "Derek Heasley"
#define MyAppURL "https://paypal.me/derekh100"
#define MyAppExeName "BigWaveRadio.exe"

[Setup]
AppId={{8F3C2A41-7B1E-4D5A-9C60-B2E44F6A21D3}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
; Per-user install: no UAC prompt, installs without admin rights.
PrivilegesRequired=lowest
DefaultDirName={localappdata}\Programs\BigWaveRadio
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
OutputDir=..\pkg
OutputBaseFilename=BigWaveRadio-Setup-0.7.1
SetupIconFile=bigwave.ico
WizardImageFile=wizard_image.bmp
WizardSmallImageFile=wizard_small.bmp
WizardStyle=modern
Compression=lzma2/ultra64
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
UninstallDisplayIcon={app}\bigwave.ico
UninstallDisplayName={#MyAppName}

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\staging\BigWaveRadio.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\staging\libunwind.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "bigwave.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\staging\README.txt"; DestDir: "{app}"; Flags: ignoreversion isreadme

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\bigwave.ico"
Name: "{group}\{cm:UninstallProgram,{#MyAppName}}"; Filename: "{uninstallexe}"; IconFilename: "{app}\bigwave.ico"
Name: "{userdesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\bigwave.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent
