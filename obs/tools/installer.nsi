Unicode true
SetCompressor /SOLID lzma
RequestExecutionLevel admin

Name "PALTO for OBS ${VERSION}"
OutFile "${OUT}"

Function .onInit
    SetShellVarContext all
    StrCpy $INSTDIR "$APPDATA\obs-studio\plugins\palto-obs"
FunctionEnd

Page directory
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

Section "Install"
    SetShellVarContext all
    SetOutPath "$INSTDIR"
    File /r "${STAGE}\*"
    WriteUninstaller "$INSTDIR\uninstall.exe"
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\PaltoObs" "DisplayName" "PALTO for OBS"
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\PaltoObs" "DisplayVersion" "${VERSION}"
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\PaltoObs" "UninstallString" "$\"$INSTDIR\uninstall.exe$\""
    WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\PaltoObs" "NoModify" 1
SectionEnd

Section "Uninstall"
    SetShellVarContext all
    RMDir /r "$INSTDIR"
    DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\PaltoObs"
SectionEnd
