$ErrorActionPreference = "Stop"

function Add-DefaultCargoPath {
  if ($null -ne (Get-Command "cargo" -ErrorAction SilentlyContinue)) {
    return
  }

  $cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $HOME ".cargo" }
  $cargoBin = Join-Path $cargoHome "bin"
  $cargoExecutable = Join-Path $cargoBin "cargo.exe"

  if (Test-Path -LiteralPath $cargoExecutable) {
    $env:Path = "$cargoBin;$env:Path"
    Write-Host "[path] Added Rust toolchain directory for this check: $cargoBin"
  }
}

function Test-Command {
  param([Parameter(Mandatory = $true)][string]$Name)
  $command = Get-Command $Name -ErrorAction SilentlyContinue
  if ($null -eq $command) {
    Write-Host "[missing] $Name"
    return $false
  }

  Write-Host "[ok] $Name -> $($command.Source)"
  return $true
}

Add-DefaultCargoPath

$ok = $true
$ok = (Test-Command "git") -and $ok
$ok = (Test-Command "node") -and $ok
$ok = (Test-Command "pnpm") -and $ok
$ok = (Test-Command "rustc") -and $ok
$ok = (Test-Command "cargo") -and $ok

if (-not $ok) {
  Write-Host ""
  Write-Host "Install missing prerequisites before generating or building the Tauri app."
  exit 1
}

Write-Host ""
Write-Host "All required prerequisites are available."
