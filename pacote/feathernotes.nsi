; Instalador do FeatherNotes para Windows (NSIS). Gerado por pacote/gerar.sh:
;   makensis -DVERSAO=0.0.1 -DEXE=caminho/feathernotes.exe -DSAIDA=caminho/instalador.exe pacote/feathernotes.nsi
; Instala para todos os usuários em "Arquivos de Programas\FeatherWorks\FeatherNotes" (pede
; administrador uma vez: só o administrador altera os .exe, e é a pasta em que o Windows e os
; antivírus mais confiam). Cria atalhos, entra no "Abrir com" dos formatos de texto (sem tomar o
; programa padrão) e registra o desinstalador em "Aplicativos instalados".
; A pasta FeatherWorks é da suíte: o Launcher fica em FeatherWorks\Launcher.

Unicode true
!include "MUI2.nsh"
!include "FileFunc.nsh"

!ifndef VERSAO
  !define VERSAO "0.0.0"
!endif

Name "FeatherNotes ${VERSAO}"
OutFile "${SAIDA}"
InstallDir "$PROGRAMFILES64\FeatherWorks\FeatherNotes"
InstallDirRegKey HKLM "Software\FeatherWorks\FeatherNotes" "Pasta"
RequestExecutionLevel admin
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
!define INICIO "Software\Microsoft\Windows\CurrentVersion\Run"

; Atalhos de todos os usuários e o registro de 64 bits (o programa é de 64 bits).
Function .onInit
  SetShellVarContext all
  SetRegView 64
FunctionEnd

Function un.onInit
  SetShellVarContext all
  SetRegView 64
FunctionEnd

Section "FeatherNotes"
  ; Até a 0.1.13 o FeatherNotes instalava só para o usuário (%LOCALAPPDATA%\Programs). Tira essa
  ; cópia, guardando o "Abrir ao iniciar o sistema" que o desinstalador antigo apaga.
  ReadRegStr $1 HKCU "${INICIO}" "FeatherNotes"
  ReadRegStr $2 HKCU "Software\FeatherWorks\FeatherNotes" "Pasta"
  StrCmp $2 "" antiga_fora
  IfFileExists "$2\desinstalar.exe" 0 antiga_fora
    ; _?= faz o desinstalador rodar no lugar e esperar (sem ele, volta antes de terminar e
    ; apagaria o "Abrir ao iniciar" já regravado); o próprio .exe e a pasta saem aqui.
    ExecWait '"$2\desinstalar.exe" /S _?=$2'
    Delete "$2\desinstalar.exe"
    RMDir "$2"
  antiga_fora:

  SetOutPath "$INSTDIR"
  ; O FeatherNotes pode estar aberto (no fundo, para os post-its): o .exe em uso não se
  ; sobrescreve, mas se renomeia; o .old sai na próxima instalação ou na desinstalação.
  Delete "$INSTDIR\feathernotes.old"
  Rename "$INSTDIR\feathernotes.exe" "$INSTDIR\feathernotes.old"
  File "/oname=feathernotes.exe" "${EXE}"
  File "icones\feathernotes.ico"
  File "/oname=LICENSE.txt" "..\LICENSE"
  WriteUninstaller "$INSTDIR\desinstalar.exe"

  CreateShortCut "$SMPROGRAMS\FeatherNotes.lnk" "$INSTDIR\feathernotes.exe" "" "$INSTDIR\feathernotes.ico"
  CreateShortCut "$DESKTOP\FeatherNotes.lnk" "$INSTDIR\feathernotes.exe" "" "$INSTDIR\feathernotes.ico"

  StrCmp $1 "" +2
    WriteRegStr HKCU "${INICIO}" "FeatherNotes" '"$INSTDIR\feathernotes.exe" --fundo'

  ; Sem formato próprio: os de texto só no "Abrir com", sem tomar o lugar do programa padrão
  ; (os mesmos de FORMATOS em src/main.rs).
  WriteRegStr HKLM "Software\Classes\FeatherNotes.Texto" "" "Texto do FeatherNotes"
  WriteRegStr HKLM "Software\Classes\FeatherNotes.Texto\DefaultIcon" "" "$INSTDIR\feathernotes.ico"
  WriteRegStr HKLM "Software\Classes\FeatherNotes.Texto\shell\open\command" "" '"$INSTDIR\feathernotes.exe" "%1"'
  WriteRegStr HKLM "Software\Classes\.txt\OpenWithProgids" "FeatherNotes.Texto" ""
  WriteRegStr HKLM "Software\Classes\.md\OpenWithProgids" "FeatherNotes.Texto" ""
  WriteRegStr HKLM "Software\Classes\.markdown\OpenWithProgids" "FeatherNotes.Texto" ""
  WriteRegStr HKLM "Software\Classes\.json\OpenWithProgids" "FeatherNotes.Texto" ""
  WriteRegStr HKLM "Software\Classes\.log\OpenWithProgids" "FeatherNotes.Texto" ""
  WriteRegStr HKLM "Software\Classes\.csv\OpenWithProgids" "FeatherNotes.Texto" ""
  WriteRegStr HKLM "Software\Classes\.ini\OpenWithProgids" "FeatherNotes.Texto" ""
  WriteRegStr HKLM "Software\Classes\.conf\OpenWithProgids" "FeatherNotes.Texto" ""
  WriteRegStr HKLM "Software\Classes\.cfg\OpenWithProgids" "FeatherNotes.Texto" ""

  WriteRegStr HKLM "Software\FeatherWorks\FeatherNotes" "Pasta" "$INSTDIR"
  WriteRegStr HKLM "${DESINSTALAR}" "DisplayName" "FeatherNotes"
  WriteRegStr HKLM "${DESINSTALAR}" "DisplayVersion" "${VERSAO}"
  WriteRegStr HKLM "${DESINSTALAR}" "Publisher" "FeatherWorks"
  ; Tamanho em "Programas e Recursos" (KB, do que foi instalado).
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKLM "${DESINSTALAR}" "EstimatedSize" "$0"
  WriteRegStr HKLM "${DESINSTALAR}" "DisplayIcon" "$INSTDIR\feathernotes.ico"
  WriteRegStr HKLM "${DESINSTALAR}" "URLInfoAbout" "https://github.com/feather-works/feathernotes"
  WriteRegStr HKLM "${DESINSTALAR}" "UninstallString" '"$INSTDIR\desinstalar.exe"'
  WriteRegStr HKLM "${DESINSTALAR}" "InstallLocation" "$INSTDIR"
  WriteRegDWORD HKLM "${DESINSTALAR}" "NoModify" 1
  WriteRegDWORD HKLM "${DESINSTALAR}" "NoRepair" 1
SectionEnd

Section "Uninstall"
  Delete "$SMPROGRAMS\FeatherNotes.lnk"
  Delete "$DESKTOP\FeatherNotes.lnk"
  Delete "$INSTDIR\feathernotes.exe"
  Delete "$INSTDIR\feathernotes.old"
  Delete "$INSTDIR\feathernotes.ico"
  Delete "$INSTDIR\LICENSE.txt"
  Delete "$INSTDIR\desinstalar.exe"
  RMDir "$INSTDIR"
  ; A pasta da suíte só sai se ficou vazia (sem outro programa nem o Launcher).
  RMDir "$INSTDIR\.."
  DeleteRegKey HKLM "Software\Classes\FeatherNotes.Texto"
  DeleteRegValue HKLM "Software\Classes\.txt\OpenWithProgids" "FeatherNotes.Texto"
  DeleteRegValue HKLM "Software\Classes\.md\OpenWithProgids" "FeatherNotes.Texto"
  DeleteRegValue HKLM "Software\Classes\.markdown\OpenWithProgids" "FeatherNotes.Texto"
  DeleteRegValue HKLM "Software\Classes\.json\OpenWithProgids" "FeatherNotes.Texto"
  DeleteRegValue HKLM "Software\Classes\.log\OpenWithProgids" "FeatherNotes.Texto"
  DeleteRegValue HKLM "Software\Classes\.csv\OpenWithProgids" "FeatherNotes.Texto"
  DeleteRegValue HKLM "Software\Classes\.ini\OpenWithProgids" "FeatherNotes.Texto"
  DeleteRegValue HKLM "Software\Classes\.conf\OpenWithProgids" "FeatherNotes.Texto"
  DeleteRegValue HKLM "Software\Classes\.cfg\OpenWithProgids" "FeatherNotes.Texto"
  DeleteRegKey HKLM "Software\FeatherWorks\FeatherNotes"
  DeleteRegKey /ifempty HKLM "Software\FeatherWorks"
  DeleteRegKey HKLM "${DESINSTALAR}"
  ; O "Abrir ao iniciar o sistema" do app (de quem desinstala).
  DeleteRegValue HKCU "${INICIO}" "FeatherNotes"
SectionEnd
