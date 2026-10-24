; NSIS hooks for the PALTO installer (bundle.windows.nsis.installerHooks).
;
; LAN access (Settings > Remote) lets phones and Companion reach PALTO, which
; Windows Firewall blocks unless a rule allows it. The installer runs per
; user, so adding the rule asks for administrator permission once; updates
; find the rule already there and do not ask again. Declining leaves PALTO
; working on this computer only, and Windows asks the first time LAN access
; is turned on instead.

!macro NSIS_HOOK_POSTINSTALL
  nsExec::ExecToStack 'netsh advfirewall firewall show rule name="PALTO"'
  Pop $0
  Pop $1
  ${If} $0 != 0
    ExecShellWait "runas" "netsh" 'advfirewall firewall add rule name="PALTO" dir=in action=allow program="$INSTDIR\${MAINBINARYNAME}.exe" enable=yes profile=private,domain' SW_HIDE
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Updates reinstall over the same folder and keep the rule.
  ${If} $UpdateMode <> 1
    nsExec::ExecToStack 'netsh advfirewall firewall show rule name="PALTO"'
    Pop $0
    Pop $1
    ${If} $0 == 0
      ExecShellWait "runas" "netsh" 'advfirewall firewall delete rule name="PALTO"' SW_HIDE
    ${EndIf}
  ${EndIf}
!macroend
