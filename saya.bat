@echo off
REM Saya installer launcher (Windows): Saya Couple nodes (the Saya Studio app is Linux-only).
REM Usage: saya install ^| verify ^| restore ^| check-compat ^| amd
set HERE=%~dp0
if "%SAYA_PYTHON%"=="" (set SAYA_PYTHON=python)
"%SAYA_PYTHON%" "%HERE%installer\saya.py" %*
