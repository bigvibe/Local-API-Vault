@echo off
rem lvault - Local API Vault shortcuts.
rem
rem This is the safe, agent-facing surface of the CLI. It reads secrets from the
rem already-unlocked Local API Vault app through its Windows Named Pipe.
rem It never accepts a master passphrase, never decrypts the vault file, and
rem never writes a secret to disk.

setlocal
set "LV=%~dp0local-api-vault-cli.exe"
set "CMD=%~1"

if /I "%CMD%"=="get" goto do_get
if /I "%CMD%"=="run" goto do_run
if /I "%CMD%"=="" goto usage
if /I "%CMD%"=="help" goto usage
if /I "%CMD%"=="--help" goto usage
if /I "%CMD%"=="-h" goto usage

echo lvault: unknown command "%CMD%"
goto usage

:do_get
"%LV%" pipe-get %2 %3 %4 %5 %6 %7 %8 %9
exit /b %ERRORLEVEL%

:do_run
"%LV%" run %2 %3 %4 %5 %6 %7 %8 %9
exit /b %ERRORLEVEL%

:usage
echo lvault - Local API Vault shortcuts
echo.
echo   lvault get ^<NAME^> [--project ^<name^>] [--environment ^<name^>]
echo   lvault run ^<profile^>
echo.
echo Reads from the unlocked Local API Vault app via Named Pipe.
echo If the vault is locked, ask the user to unlock it.
echo Never ask for, and never pass, a master passphrase.
if /I "%CMD%"=="help" exit /b 0
if /I "%CMD%"=="--help" exit /b 0
if /I "%CMD%"=="-h" exit /b 0
exit /b 1