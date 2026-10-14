; Inno Setup script for the Inkframe installer.
;
; Built by `cargo xtask bundle-windows`, which gathers the files and passes
; in the values below with /D. To build by hand, run ISCC.exe with the same
; defines (see xtask/src/windows.rs).
;
;   AppVersion   version from Cargo.toml, e.g. 0.1.0
;   SourceDir    folder holding inkframe.exe, ffmpeg.exe (optional), licenses\
;   IconFile     assets\icon\inkframe.ico
;   OutputDir    where setup.exe is written
;   OutputName   setup.exe's name, without the extension

[Setup]
; Identifies Inkframe to Windows across versions, so a newer installer
; upgrades the existing install instead of adding a second one. Never change
; it. (The doubled brace is how Inno Setup writes a literal "{".)
AppId={{037584DB-648F-4DEC-AC79-70B3DC082D2E}
AppName=Inkframe
AppVersion={#AppVersion}
AppPublisher=Inkframe contributors
AppPublisherURL=https://github.com/WhateverTrevor/inkframe
AppSupportURL=https://github.com/WhateverTrevor/inkframe/issues

; Install for the current user by default, which needs no administrator
; rights. The first page of the installer offers an all-users install too,
; and {autopf} follows that choice (AppData\Local\Programs, or Program Files).
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
DefaultDirName={autopf}\Inkframe
DisableProgramGroupPage=yes

; x64compatible also lets the x64 build install on Windows on Arm, which
; runs it under emulation.
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

SetupIconFile={#IconFile}
UninstallDisplayIcon={app}\inkframe.exe
WizardStyle=modern
OutputDir={#OutputDir}
OutputBaseFilename={#OutputName}
Compression=lzma2/max
SolidCompression=yes
; Lets Explorer pick up the .inkf file icon without a restart.
ChangesAssociations=yes

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "associate"; Description: "Open .inkf project files with Inkframe"; GroupDescription: "File types:"

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs

[Icons]
Name: "{autoprograms}\Inkframe"; Filename: "{app}\inkframe.exe"
Name: "{autodesktop}\Inkframe"; Filename: "{app}\inkframe.exe"; Tasks: desktopicon

; Double-clicking a .inkf file runs `inkframe.exe "path\to\file.inkf"`,
; which opens it, the same as passing a file on the command line.
; HKA writes to the current user's or the machine's registry to match the
; install type, and the uninsdelete flags remove these entries again.
[Registry]
Root: HKA; Subkey: "Software\Classes\.inkf"; ValueType: string; ValueName: ""; ValueData: "Inkframe.Project"; Flags: uninsdeletevalue; Tasks: associate
Root: HKA; Subkey: "Software\Classes\Inkframe.Project"; ValueType: string; ValueName: ""; ValueData: "Inkframe Project"; Flags: uninsdeletekey; Tasks: associate
Root: HKA; Subkey: "Software\Classes\Inkframe.Project\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\inkframe.exe,0"; Tasks: associate
Root: HKA; Subkey: "Software\Classes\Inkframe.Project\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\inkframe.exe"" ""%1"""; Tasks: associate

[Run]
Filename: "{app}\inkframe.exe"; Description: "{cm:LaunchProgram,Inkframe}"; Flags: nowait postinstall skipifsilent
