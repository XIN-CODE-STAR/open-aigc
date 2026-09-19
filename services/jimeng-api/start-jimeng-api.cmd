@echo off
cd /d "%~dp0"
"D:\X\nodejs\node.exe" --enable-source-maps --no-node-snapshot dist/index.js >> "%~dp0logs\service-runtime.log" 2>&1
