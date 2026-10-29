; Hooks for Tauri's NSIS installer (bundle.windows.nsis.installerHooks).

; The data folder the player picked instead of the default one, if any.
Var BloogBaseDataDir

; The uninstaller's "delete app data" removes only the default folder in
; %APPDATA%, which also holds the pointer to a data folder the player picked
; elsewhere (paths.rs, data-location.txt). Read the pointer before it goes.
!macro NSIS_HOOK_PREUNINSTALL
  StrCpy $BloogBaseDataDir ""
  SetShellVarContext current
  ClearErrors
  FileOpen $0 "$APPDATA\${BUNDLEID}\data-location.txt" r
  ${IfNot} ${Errors}
    ; The app writes it as UTF-8; FileRead would take it for ANSI and break
    ; paths with umlauts, so read the bytes and convert them.
    FileSeek $0 0 END $1
    FileSeek $0 0 SET
    IntOp $2 $1 + 1
    System::Alloc $2
    Pop $2
    System::Call "kernel32::ReadFile(p r0, p r2, i r1, *i .r3, p 0)"
    FileClose $0
    System::Call "kernel32::MultiByteToWideChar(i 65001, i 0, p r2, i -1, w .r4, i ${NSIS_MAX_STRLEN})"
    System::Free $2
    StrCpy $BloogBaseDataDir $4
  ${EndIf}
!macroend

; Same conditions as the template's own app data removal. Only what BloogBase
; puts there goes, so files of the player's own in that folder stay, and only
; if the folder still holds BloogBase's database.
!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
  ${AndIf} $BloogBaseDataDir != ""
  ${AndIf} ${FileExists} "$BloogBaseDataDir\bloogbase.db"
    RMDir /r "$BloogBaseDataDir\runtimes"
    RMDir /r "$BloogBaseDataDir\library"
    RMDir /r "$BloogBaseDataDir\mods"
    RMDir /r "$BloogBaseDataDir\profiles"
    RMDir /r "$BloogBaseDataDir\backups"
    RMDir /r "$BloogBaseDataDir\cache"
    RMDir /r "$BloogBaseDataDir\configs"
    RMDir /r "$BloogBaseDataDir\logs"
    Delete "$BloogBaseDataDir\bloogbase.db"
    Delete "$BloogBaseDataDir\bloogbase.db-wal"
    Delete "$BloogBaseDataDir\bloogbase.db-shm"
    RMDir "$BloogBaseDataDir"
  ${EndIf}
!macroend
