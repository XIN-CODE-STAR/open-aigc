@echo off
cd /d "%~dp0"
"D:/X/nodejs/node.exe" --enable-source-maps --no-node-snapshot dist/index.js >> logs/service-runtime.log 2>&1
