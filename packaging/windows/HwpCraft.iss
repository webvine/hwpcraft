; HwpCraft Windows installer (Inno Setup 6).
; Build with scripts/build-installer.ps1, which stages the files and passes
; /DAppVersion, /DSourceDir and /DOutputDir. Do not run ISCC on this file directly.

#ifndef AppVersion
  #error Pass /DAppVersion=x.xx (scripts/build-installer.ps1 does this)
#endif
#ifndef SourceDir
  #error Pass /DSourceDir=<staged files>
#endif
#ifndef OutputDir
  #define OutputDir "."
#endif

#define AppName "HwpCraft"
#define AppExe "hwpcraft.exe"
#define AppCli "hwpcraft-cli.exe"

[Setup]
AppId={{7D3B1A64-5C2E-4F0B-9A6C-1E8C2F4D7A13}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
VersionInfoVersion={#AppVersion}.0.0
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
UninstallDisplayIcon={app}\hwpcraft.ico
UninstallDisplayName={#AppName} {#AppVersion}
LicenseFile={#SourceDir}\LICENSE
SetupIconFile={#SourceDir}\hwpcraft.ico
OutputDir={#OutputDir}
OutputBaseFilename={#AppName}-{#AppVersion}-windows-x64-setup
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
; Per-user install by default (no UAC); the dialog lets an administrator choose all users.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ChangesAssociations=yes
DisableProgramGroupPage=yes
CloseApplications=yes

[Languages]
Name: "korean"; MessagesFile: "compiler:Languages\Korean.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
korean.DesktopIcon=바탕 화면에 바로 가기 만들기
korean.AssocHwp=.hwp / .hwpx 파일을 HwpCraft로 열기 (한글이 설치돼 있으면 연결이 바뀝니다)
korean.OpenSample=시작하기 문서 열기
english.DesktopIcon=Create a desktop shortcut
english.AssocHwp=Open .hwp / .hwpx files with HwpCraft (changes the association if Hangul is installed)
english.OpenSample=Open the getting-started document

[Tasks]
Name: "desktopicon"; Description: "{cm:DesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "assochwp"; Description: "{cm:AssocHwp}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceDir}\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\{#AppCli}"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\hwpcraft.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\VERSION"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\THIRD-PARTY-NOTICES.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\THIRD-PARTY-DEPENDENCIES.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\LICENSE-*.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\samples\*"; DestDir: "{app}\samples"; Flags: ignoreversion recursesubdirs

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExe}"; IconFilename: "{app}\hwpcraft.ico"; WorkingDir: "{app}"
Name: "{group}\{cm:OpenSample}"; Filename: "{app}\{#AppExe}"; Parameters: """{app}\samples\HwpCraft-시작하기.hwpx"""; IconFilename: "{app}\hwpcraft.ico"; WorkingDir: "{app}"
Name: "{group}\{cm:UninstallProgram,{#AppName}}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; IconFilename: "{app}\hwpcraft.ico"; WorkingDir: "{app}"; Tasks: desktopicon

[Registry]
; File associations, only when the task is chosen. HKA = HKLM for all users, HKCU per user.
Root: HKA; Subkey: "Software\Classes\.hwp"; ValueType: string; ValueName: ""; ValueData: "HwpCraft.Document"; Flags: uninsdeletevalue; Tasks: assochwp
Root: HKA; Subkey: "Software\Classes\.hwpx"; ValueType: string; ValueName: ""; ValueData: "HwpCraft.Document"; Flags: uninsdeletevalue; Tasks: assochwp
Root: HKA; Subkey: "Software\Classes\HwpCraft.Document"; ValueType: string; ValueName: ""; ValueData: "HWP Document (HwpCraft)"; Flags: uninsdeletekey; Tasks: assochwp
Root: HKA; Subkey: "Software\Classes\HwpCraft.Document\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\hwpcraft.ico"; Tasks: assochwp
Root: HKA; Subkey: "Software\Classes\HwpCraft.Document\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""; Tasks: assochwp

[Run]
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchProgram,{#AppName}}"; WorkingDir: "{app}"; Flags: nowait postinstall skipifsilent
