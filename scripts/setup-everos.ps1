#Requires -Version 5.1
<#
.SYNOPSIS
    安装和初始化 EverOS 记忆服务。
.DESCRIPTION
    检查 Python 3.12+，安装 everos 包，初始化配置目录。
    EverOS 数据存储在 <workspace>/everos-memory/ 下。
.PARAMETER WorkspacePath
    AIGC Studio 工作空间路径。
.PARAMETER Port
    EverOS 服务端口，默认 18000。
#>
param(
    [Parameter(Mandatory = $true)]
    [string]$WorkspacePath,

    [int]$Port = 18000
)

$ErrorActionPreference = "Stop"

$memoryRoot = Join-Path $WorkspacePath "everos-memory"

Write-Host "=== AIGC Studio EverOS 记忆服务安装 ===" -ForegroundColor Cyan
Write-Host "工作空间: $WorkspacePath"
Write-Host "记忆目录: $memoryRoot"
Write-Host "服务端口: $Port"
Write-Host ""

# 1. 检查 Python 版本
Write-Host "[1/4] 检查 Python..." -ForegroundColor Yellow
try {
    $pythonVersion = python --version 2>&1
    if ($pythonVersion -match "Python (\d+)\.(\d+)") {
        $major = [int]$Matches[1]
        $minor = [int]$Matches[2]
        if ($major -lt 3 -or ($major -eq 3 -and $minor -lt 12)) {
            Write-Host "  需要 Python 3.12+，当前版本: $pythonVersion" -ForegroundColor Red
            Write-Host "  请安装 Python 3.12 或更高版本: https://www.python.org/downloads/" -ForegroundColor Red
            exit 1
        }
        Write-Host "  Python 版本: $pythonVersion" -ForegroundColor Green
    }
} catch {
    Write-Host "  未找到 Python，请安装 Python 3.12+" -ForegroundColor Red
    exit 1
}

# 2. 安装 everos
Write-Host "[2/4] 安装 everos..." -ForegroundColor Yellow
pip install everos --quiet
if ($LASTEXITCODE -ne 0) {
    Write-Host "  pip install 失败，尝试 pip3..." -ForegroundColor Yellow
    pip3 install everos --quiet
    if ($LASTEXITCODE -ne 0) {
        Write-Host "  安装失败，请手动运行: pip install everos" -ForegroundColor Red
        exit 1
    }
}
Write-Host "  everos 安装成功" -ForegroundColor Green

# 3. 初始化配置
Write-Host "[3/4] 初始化配置..." -ForegroundColor Yellow
if (-not (Test-Path $memoryRoot)) {
    New-Item -ItemType Directory -Path $memoryRoot -Force | Out-Null
}

$everosToml = Join-Path $memoryRoot "everos.toml"
if (-not (Test-Path $everosToml)) {
    everos init --root $memoryRoot
    if ($LASTEXITCODE -ne 0) {
        Write-Host "  初始化失败" -ForegroundColor Red
        exit 1
    }
    Write-Host "  配置已初始化: $everosToml" -ForegroundColor Green
} else {
    Write-Host "  配置已存在，跳过初始化" -ForegroundColor Green
}

# 4. 验证安装
Write-Host "[4/4] 验证安装..." -ForegroundColor Yellow
$everosPath = Get-Command everos -ErrorAction SilentlyContinue
if ($everosPath) {
    Write-Host "  everos 命令可用: $($everosPath.Source)" -ForegroundColor Green
} else {
    Write-Host "  警告: everos 命令不在 PATH 中" -ForegroundColor Yellow
}

Write-Host ""
Write-Host "=== 安装完成 ===" -ForegroundColor Green
Write-Host ""
Write-Host "下一步:" -ForegroundColor Cyan
Write-Host "  1. 编辑 $everosToml 配置 LLM 和 Embedding API Key"
Write-Host "  2. 在 AIGC Studio 设置中启用 EverOS 记忆服务"
Write-Host "  3. EverOS 将在应用启动时自动运行"
