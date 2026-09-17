@echo off
setlocal
set "SCRIPT_DIR=%~dp0"

REM 1. Prefer existing release binary in target/release or ctrl-cli/target/release
if exist "%SCRIPT_DIR%target\release\ctrl-cli.exe" (
    "%SCRIPT_DIR%target\release\ctrl-cli.exe" tui %*
    exit /b %ERRORLEVEL%
)
if exist "%SCRIPT_DIR%ctrl-cli\target\release\ctrl-cli.exe" (
    "%SCRIPT_DIR%ctrl-cli\target\release\ctrl-cli.exe" tui %*
    exit /b %ERRORLEVEL%
)

REM 2. Check debug binary
if exist "%SCRIPT_DIR%target\debug\ctrl-cli.exe" (
    "%SCRIPT_DIR%target\debug\ctrl-cli.exe" tui %*
    exit /b %ERRORLEVEL%
)
if exist "%SCRIPT_DIR%ctrl-cli\target\debug\ctrl-cli.exe" (
    "%SCRIPT_DIR%ctrl-cli\target\debug\ctrl-cli.exe" tui %*
    exit /b %ERRORLEVEL%
)

REM 3. Fallback to running via cargo
if exist "%SCRIPT_DIR%Cargo.toml" (
    cargo run --manifest-path "%SCRIPT_DIR%Cargo.toml" --bin ctrl-cli -- tui %*
) else (
    cargo run --manifest-path "%SCRIPT_DIR%ctrl-cli\Cargo.toml" --bin ctrl-cli -- tui %*
)
exit /b %ERRORLEVEL%
