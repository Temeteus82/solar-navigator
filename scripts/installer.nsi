; NSIS installer script for Solar Navigator.
;
; Driven by scripts/package_windows_installer.ps1, which stages both the
; SPICE-enabled and fallback binaries and passes the staging path, version,
; and output filename via /D defines. Requires NSIS 3.x with bundled MUI2.

Unicode true

!ifndef APP_VERSION
  !define APP_VERSION "0.0.0"
!endif
!ifndef STAGE_DIR
  !error "STAGE_DIR must be defined (path to the staged install tree)."
!endif
!ifndef OUTPUT_FILE
  !error "OUTPUT_FILE must be defined (full path of the .exe to produce)."
!endif

!define APP_NAME "Solar Navigator"
!define APP_SLUG "solar-navigator"
!define APP_PUBLISHER "Solar Navigator"
!define APP_URL "https://github.com/Temeteus82/solar-navigator"
!define APP_EXE "${APP_SLUG}.exe"
!define APP_REG_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_NAME}"

Name "${APP_NAME} ${APP_VERSION}"
OutFile "${OUTPUT_FILE}"
InstallDir "$PROGRAMFILES64\${APP_NAME}"
InstallDirRegKey HKLM "${APP_REG_KEY}" "InstallLocation"
RequestExecutionLevel admin
SetCompressor /SOLID lzma
ShowInstDetails show
ShowUninstDetails show

!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"
!include "nsDialogs.nsh"

!define MUI_ABORTWARNING
!define MUI_ICON   "..\assets\icon\AppIcon.ico"
!define MUI_UNICON "..\assets\icon\AppIcon.ico"

; -- State -------------------------------------------------------------------
Var SimulationMode      ; "spice" or "fallback"
Var RadioSpice
Var RadioFallback
Var CheckDesktop
Var CreateDesktopShortcut

; -- Pages -------------------------------------------------------------------
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "..\LICENSE"
Page custom PageSimulationMode PageSimulationModeLeave
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\${APP_EXE}"
!define MUI_FINISHPAGE_RUN_TEXT "Launch ${APP_NAME}"
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_WELCOME
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_UNPAGE_FINISH

!insertmacro MUI_LANGUAGE "English"

VIProductVersion "0.0.0.0"
VIAddVersionKey "ProductName"     "${APP_NAME}"
VIAddVersionKey "CompanyName"     "${APP_PUBLISHER}"
VIAddVersionKey "FileDescription" "${APP_NAME} Installer"
VIAddVersionKey "FileVersion"     "${APP_VERSION}"
VIAddVersionKey "ProductVersion"  "${APP_VERSION}"
VIAddVersionKey "LegalCopyright"  "${APP_PUBLISHER}"

; -- Custom page: simulation mode picker ------------------------------------
Function PageSimulationMode
  !insertmacro MUI_HEADER_TEXT "Simulation Mode" "Choose how Solar Navigator computes body positions."

  nsDialogs::Create 1018
  Pop $0
  ${If} $0 == error
    Abort
  ${EndIf}

  ${NSD_CreateLabel} 0 0 100% 28u "Solar Navigator can run with NASA's SPICE ephemerides for accurate body positions, or with built-in analytic Keplerian orbits. The SPICE option is more accurate but downloads ~50 MB of NAIF kernels with the install."
  Pop $0

  ${NSD_CreateRadioButton} 0 36u 100% 12u "Realistic mode (SPICE) — accurate ephemerides, larger install"
  Pop $RadioSpice

  ${NSD_CreateRadioButton} 0 52u 100% 12u "Fallback mode (analytic) — smaller, simpler Keplerian orbits"
  Pop $RadioFallback

  ${NSD_Check} $RadioSpice

  ${NSD_CreateCheckbox} 0 80u 100% 12u "Also create a shortcut on the desktop"
  Pop $CheckDesktop
  ${NSD_Check} $CheckDesktop

  nsDialogs::Show
FunctionEnd

Function PageSimulationModeLeave
  ${NSD_GetState} $RadioSpice $0
  ${If} $0 == ${BST_CHECKED}
    StrCpy $SimulationMode "spice"
  ${Else}
    StrCpy $SimulationMode "fallback"
  ${EndIf}

  ${NSD_GetState} $CheckDesktop $0
  ${If} $0 == ${BST_CHECKED}
    StrCpy $CreateDesktopShortcut "1"
  ${Else}
    StrCpy $CreateDesktopShortcut "0"
  ${EndIf}
FunctionEnd

; -- Install -----------------------------------------------------------------
Section "Install" SecMain
  SectionIn RO
  SetOutPath "$INSTDIR"

  ; Pick binary based on user's simulation-mode choice
  ${If} $SimulationMode == "spice"
    DetailPrint "Installing realistic (SPICE) build."
    File /oname=${APP_EXE} "${STAGE_DIR}\solar-navigator-spice.exe"

    ; SPICE kernels
    SetOutPath "$INSTDIR\assets\spice"
    File /r "${STAGE_DIR}\spice-kernels\*.*"
  ${Else}
    DetailPrint "Installing fallback (analytic) build."
    File /oname=${APP_EXE} "${STAGE_DIR}\solar-navigator-fallback.exe"
  ${EndIf}

  ; App icon (used by the running app to set the window icon)
  SetOutPath "$INSTDIR\assets\icon"
  File /r "${STAGE_DIR}\icon\*.*"

  ; Texture download scripts (also kicked off in the background below)
  SetOutPath "$INSTDIR\scripts"
  File "${STAGE_DIR}\download_textures_solar_system_scope.ps1"
  File "${STAGE_DIR}\download_textures_minor_bodies_science.ps1"

  SetOutPath "$INSTDIR"

  ; Start Menu shortcuts
  CreateDirectory "$SMPROGRAMS\${APP_NAME}"
  CreateShortcut  "$SMPROGRAMS\${APP_NAME}\${APP_NAME}.lnk" "$INSTDIR\${APP_EXE}" "" "$INSTDIR\${APP_EXE}" 0
  CreateShortcut  "$SMPROGRAMS\${APP_NAME}\Uninstall ${APP_NAME}.lnk" "$INSTDIR\Uninstall.exe"

  ; Optional desktop shortcut (chosen on the simulation-mode page)
  ${If} $CreateDesktopShortcut == "1"
    CreateShortcut "$DESKTOP\${APP_NAME}.lnk" "$INSTDIR\${APP_EXE}" "" "$INSTDIR\${APP_EXE}" 0
  ${EndIf}

  ; Uninstaller
  WriteUninstaller "$INSTDIR\Uninstall.exe"

  ; Add/Remove Programs
  WriteRegStr   HKLM "${APP_REG_KEY}" "DisplayName"          "${APP_NAME}"
  WriteRegStr   HKLM "${APP_REG_KEY}" "DisplayVersion"       "${APP_VERSION}"
  WriteRegStr   HKLM "${APP_REG_KEY}" "Publisher"            "${APP_PUBLISHER}"
  WriteRegStr   HKLM "${APP_REG_KEY}" "URLInfoAbout"         "${APP_URL}"
  WriteRegStr   HKLM "${APP_REG_KEY}" "InstallLocation"      "$INSTDIR"
  WriteRegStr   HKLM "${APP_REG_KEY}" "DisplayIcon"          "$INSTDIR\${APP_EXE}"
  WriteRegStr   HKLM "${APP_REG_KEY}" "UninstallString"      "$\"$INSTDIR\Uninstall.exe$\""
  WriteRegStr   HKLM "${APP_REG_KEY}" "QuietUninstallString" "$\"$INSTDIR\Uninstall.exe$\" /S"
  WriteRegStr   HKLM "${APP_REG_KEY}" "SimulationMode"       "$SimulationMode"
  WriteRegDWORD HKLM "${APP_REG_KEY}" "NoModify" 1
  WriteRegDWORD HKLM "${APP_REG_KEY}" "NoRepair" 1

  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  WriteRegDWORD HKLM "${APP_REG_KEY}" "EstimatedSize" "$0"

  ; Kick off background texture download. Fire-and-forget; user can launch
  ; the app immediately. Missing textures degrade to plain colours, and
  ; will appear on the next app launch once the download finishes.
  DetailPrint "Starting background texture download..."
  Exec '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File "$INSTDIR\scripts\download_textures_solar_system_scope.ps1"'
SectionEnd

; -- Uninstall ---------------------------------------------------------------
Section "Uninstall"
  Delete "$INSTDIR\${APP_EXE}"
  RMDir /r "$INSTDIR\assets"
  RMDir /r "$INSTDIR\scripts"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"

  Delete "$SMPROGRAMS\${APP_NAME}\${APP_NAME}.lnk"
  Delete "$SMPROGRAMS\${APP_NAME}\Uninstall ${APP_NAME}.lnk"
  RMDir  "$SMPROGRAMS\${APP_NAME}"
  Delete "$DESKTOP\${APP_NAME}.lnk"

  DeleteRegKey HKLM "${APP_REG_KEY}"
SectionEnd
