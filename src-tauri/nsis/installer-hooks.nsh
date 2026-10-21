!include LogicLib.nsh
!include FileFunc.nsh

Var HuimengInstallerLogDir
Var HuimengInstallerVersion
Var HuimengInstallerFailure
Var HuimengInstallerProbeError
Var HuimengInstallerAttempts

; Hooks 在 Tauri 的版本常量、语言定义之前载入，运行时参数由 PREINSTALL 初始化。
LangString HuimengInstallFailed 2052 "慧梦安装失败。请关闭慧梦和占用其文件的程序后重试，或下载新版安装包重新安装。"
LangString HuimengInstallFailed 1033 "Huimeng could not be installed. Close Huimeng and any programs using its files, then try again or download the new installer."

Function HuimengWriteInstallLog
  Exch $R9
  Push $R0
  Push $R1
  Push $R2
  Push $R3
  Push $R4
  Push $R5
  Push $R6
  Push $R7

  StrCmp $HuimengInstallerLogDir "" done
  CreateDirectory "$HuimengInstallerLogDir"
  ClearErrors
  FileOpen $R7 "$HuimengInstallerLogDir\installer.log" a
  IfErrors log_failed
  ${GetTime} "" "L" $R0 $R1 $R2 $R3 $R4 $R5 $R6
  FileWrite $R7 "$R2-$R1-$R0T$R4:$R5:$R6 phase=$R9 version=$HuimengInstallerVersion reason=$HuimengInstallerFailure win_error=$HuimengInstallerProbeError attempts=$HuimengInstallerAttempts$\r$\n"
  FileClose $R7
  Goto done

  log_failed:
    DetailPrint "无法写入安装阶段日志。"
  done:
    Pop $R7
    Pop $R6
    Pop $R5
    Pop $R4
    Pop $R3
    Pop $R2
    Pop $R1
    Pop $R0
    Pop $R9
    ClearErrors
FunctionEnd

; Silent 模式也执行此回调。保留可见错误，避免退出旧进程后安装失败却没有反馈。
Function .onInstFailed
  Push "failed"
  Call HuimengWriteInstallLog
  MessageBox MB_OK|MB_ICONSTOP "$(HuimengInstallFailed)"
FunctionEnd

!macro NSIS_HOOK_PREINSTALL
  ; NSIS 默认在 /S 下忽略无法覆盖的 File，导致错误地写新版本并重启旧 exe。
  ; 禁止跳过：实际复制失败时必须终止，后面的版本注册表和 /R 都不会执行。
  AllowSkipFiles off
  StrCpy $HuimengInstallerLogDir "$LOCALAPPDATA\${BUNDLEID}\logs"
  StrCpy $HuimengInstallerVersion "${VERSION}"
  StrCpy $HuimengInstallerFailure "copy"
  StrCpy $HuimengInstallerProbeError 0
  StrCpy $HuimengInstallerAttempts 0
  Push "started"
  Call HuimengWriteInstallLog

  ; 保留手动覆盖安装时原有的关闭应用提示，随后再等待进程解除文件映射。
  ; Tauri 模板在 PREINSTALL 后还会再检查一次，应用已退出时该检查为空操作。
  !insertmacro CheckIfAppIsRunning "${MAINBINARYNAME}.exe" "${PRODUCTNAME}"

  Push $0
  IfFileExists "$INSTDIR\${MAINBINARYNAME}.exe" 0 huimeng_probe_done
  huimeng_probe:
    ; OPEN_EXISTING 只检测写权限，不截断、不改写旧文件；共享读/写/删除以免阻塞他人。
    ; 等待旧进程真正解除映射，释放即继续，最多 20 次、间隔 250ms。
    System::Call 'kernel32::CreateFileW(w "$INSTDIR\${MAINBINARYNAME}.exe", i 0x40000000, i 7, p 0, i 3, i 0x80, p 0) p .r0 ?e'
    Pop $HuimengInstallerProbeError
    IntOp $HuimengInstallerAttempts $HuimengInstallerAttempts + 1
    ${If} $0 P<> -1
      System::Call 'kernel32::CloseHandle(p r0)'
      StrCpy $HuimengInstallerProbeError 0
      Push "file-ready"
      Call HuimengWriteInstallLog
      Goto huimeng_probe_done
    ${EndIf}
    ${If} $HuimengInstallerAttempts >= 20
      StrCpy $HuimengInstallerFailure "file-unavailable"
      Pop $0
      Abort
    ${EndIf}
    Sleep 250
    Goto huimeng_probe
  huimeng_probe_done:
    Pop $0
    ClearErrors
!macroend

!macro NSIS_HOOK_POSTINSTALL
  StrCpy $HuimengInstallerFailure ""
  Push "installed"
  Call HuimengWriteInstallLog
!macroend
