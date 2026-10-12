; Custom NSIS hooks for Local API Vault.
;
; The optional "start at login" feature writes a per-user registry value under
; HKCU\Software\Microsoft\Windows\CurrentVersion\Run. Remove it on uninstall so
; Windows does not keep trying to launch a deleted executable.

!macro NSIS_HOOK_PREUNINSTALL
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "LocalApiVault"
!macroend
