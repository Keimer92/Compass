@echo off
echo test1
where cargo >nul 2>nul
if %errorlevel% neq 0 (
echo error
)
echo test2
