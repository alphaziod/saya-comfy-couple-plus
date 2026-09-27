@echo off
REM Saya Couple installer launcher (Windows). Usage: saya install ^| verify ^| restore ^| check-compat ^| amd
set HERE=%~dp0
if "%SAYA_PYTHON%"=="" (set SAYA_PYTHON=python)
"%SAYA_PYTHON%" "%HERE%installer\saya.py" %*
