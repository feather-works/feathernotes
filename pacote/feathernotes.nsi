; Instalador do FeatherNotes para Windows (NSIS). Gerado por pacote/gerar.sh:
;   makensis -DVERSAO=0.0.1 -DEXE=caminho/feathernotes.exe -DSAIDA=caminho/instalador.exe pacote/feathernotes.nsi
; Instala por usuário (sem pedir administrador), cria atalhos
; e registra o desinstalador em "Aplicativos instalados".

Unicode true
!include "MUI2.nsh"

!ifndef VERSAO
  !define VERSAO "0.0.0"
!endif

Name "FeatherNotes ${VERSAO}"
OutFile "${SAIDA}"
InstallDir "$LOCALAPPDATA\Programs\FeatherNotes"
InstallDirRegKey HKCU "Software\FeatherWorks\FeatherNotes" "Pasta"
RequestExecutionLevel user
SetCompressor /SOLID lzma

!define MUI_ICON "icones\feathernotes.ico"
!define MUI_UNICON "icones\feathernotes.ico"
!define MUI_FINISHPAGE_RUN "$INSTDIR\feathernotes.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Abrir o FeatherNotes"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "..\LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "PortugueseBR"

!define DESINSTALAR "Software\Microsoft\Windows\CurrentVersion\Uninstall\FeatherNotes"

Section "FeatherNotes"
  SetOutPath "$INSTDIR"
  File "/oname=feathernotes.exe" "${EXE}"
  File "icones\feathernotes.ico"
  File "/oname=LICENSE.txt" "..\LICENSE"
  WriteUninstaller "$INSTDIR\desinstalar.exe"

  CreateShortCut "$SMPROGRAMS\FeatherNotes.lnk" "$INSTDIR\feathernotes.exe" "" "$INSTDIR\feathernotes.ico"
  CreateShortCut "$DESKTOP\FeatherNotes.lnk" "$INSTDIR\feathernotes.exe" "" "$INSTDIR\feathernotes.ico"

  WriteRegStr HKCU "Software\FeatherWorks\FeatherNotes" "Pasta" "$INSTDIR"
  WriteRegStr HKCU "${DESINSTALAR}" "DisplayName" "FeatherNotes"
  WriteRegStr HKCU "${DESINSTALAR}" "DisplayVersion" "${VERSAO}"
  WriteRegStr HKCU "${DESINSTALAR}" "Publisher" "FeatherWorks"
  WriteRegStr HKCU "${DESINSTALAR}" "DisplayIcon" "$INSTDIR\feathernotes.ico"
  WriteRegStr HKCU "${DESINSTALAR}" "URLInfoAbout" "https://github.com/feather-works/feathernotes"
  WriteRegStr HKCU "${DESINSTALAR}" "UninstallString" '"$INSTDIR\desinstalar.exe"'
  WriteRegDWORD HKCU "${DESINSTALAR}" "NoModify" 1
  WriteRegDWORD HKCU "${DESINSTALAR}" "NoRepair" 1
SectionEnd

Section "Uninstall"
  Delete "$SMPROGRAMS\FeatherNotes.lnk"
  Delete "$DESKTOP\FeatherNotes.lnk"
  Delete "$INSTDIR\feathernotes.exe"
  Delete "$INSTDIR\feathernotes.ico"
  Delete "$INSTDIR\LICENSE.txt"
  Delete "$INSTDIR\desinstalar.exe"
  RMDir "$INSTDIR"
  DeleteRegKey HKCU "Software\FeatherWorks\FeatherNotes"
  DeleteRegKey HKCU "${DESINSTALAR}"
  ; O "Abrir ao iniciar o sistema" do app.
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "FeatherNotes"
SectionEnd
