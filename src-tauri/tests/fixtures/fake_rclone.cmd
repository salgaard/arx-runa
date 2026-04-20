@echo off
setlocal

set "command=%~1"
if /I "%command%"=="config" (
  if /I "%~2"=="create" (
    echo If your browser doesn't open automatically, go to the following link: https://example.com/oauth
    exit /B 0
  )
  if /I "%~2"=="dump" (
    echo {"remote":{"type":"drive"}}
    exit /B 0
  )
  >&2 echo unknown config subcommand
  exit /B 2
)

if /I "%command%"=="ok" (
  if "%~2"=="" (
    echo ok
  ) else (
    echo %~2
  )
  exit /B 0
)

if /I "%command%"=="status" (
  if "%~3"=="" (
    >&2 echo error
  ) else (
    >&2 echo %~3
  )
  if "%~2"=="" (
    exit /B 1
  ) else (
    exit /B %~2
  )
)

if /I "%command%"=="sleep" (
  powershell -NoProfile -Command "Start-Sleep -Seconds 10"
  exit /B 0
)

>&2 echo unknown command
exit /B 2
