!macro NSIS_HOOK_POSTINSTALL
  nsExec::ExecToStack 'netsh advfirewall firewall show rule name="PALTO"'
  Pop $0
  Pop $1
  ${If} $0 != 0
    ExecShellWait "runas" "netsh" 'advfirewall firewall add rule name="PALTO" dir=in action=allow program="$INSTDIR\${MAINBINARYNAME}.exe" enable=yes profile=private,domain' SW_HIDE
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    nsExec::ExecToStack 'netsh advfirewall firewall show rule name="PALTO"'
    Pop $0
    Pop $1
    ${If} $0 == 0
      ExecShellWait "runas" "netsh" 'advfirewall firewall delete rule name="PALTO"' SW_HIDE
    ${EndIf}
  ${EndIf}
!macroend
