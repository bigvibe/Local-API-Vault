@echo off
rem lvault - Local API Vault shortcuts.
rem
rem This is the safe, agent-facing surface of the CLI. `get` and `run` read
rem secrets from the already-unlocked Local API Vault app through its Windows
rem Named Pipe. They never accept a master passphrase, never decrypt the vault
rem file, and never write a secret to disk.
rem
rem `verify` is the exception, and stays read-only: it never asks for a
rem passphrase, never prints a secret, and never writes anything. With
rem --identity it decrypts once, in memory, to confirm that the recovery
rem identity still opens this vault.

setlocal
set "LV=%~dp0local-api-vault-cli.exe"
set "CMD=%~1"

if /I "%CMD%"=="get" goto do_get
if /I "%CMD%"=="run" goto do_run
if /I "%CMD%"=="verify" goto do_verify
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

:do_verify
"%LV%" verify %2 %3 %4 %5 %6 %7 %8 %9
exit /b %ERRORLEVEL%

:usage
echo lvault - Local API Vault shortcuts
echo.
echo   lvault get ^<NAME^> [--project ^<name^>] [--environment ^<name^>]
echo   lvault run ^<profile^>
echo   lvault verify [--vault ^<path^>] [--identity ^<recovery identity file^>]
echo.
echo get and run read from the unlocked Local API Vault app via Named Pipe.
echo If the vault is locked, ask the user to unlock it.
echo Never ask for, and never pass, a master passphrase.
echo.
echo verify checks the vault files read-only: it never asks for a passphrase,
echo never prints a secret, and never changes any file.
if /I "%CMD%"=="help" exit /b 0
if /I "%CMD%"=="--help" exit /b 0
if /I "%CMD%"=="-h" exit /b 0
exit /b 1
