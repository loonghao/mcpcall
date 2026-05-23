[CmdletBinding()]
param(
    [string] $Repo = $(if ($env:MCPCALL_REPO) { $env:MCPCALL_REPO } else { "loonghao/mcpcall" }),
    [string] $Version = $(if ($env:MCPCALL_VERSION) { $env:MCPCALL_VERSION } else { "latest" }),
    [string] $InstallDir = $(if ($env:MCPCALL_INSTALL_DIR) { $env:MCPCALL_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "mcpcall\bin" })
)

$ErrorActionPreference = "Stop"

$artifact = "mcpcall-windows-x86_64.exe"
if ($Version -eq "latest") {
    $url = "https://github.com/$Repo/releases/latest/download/$artifact"
} else {
    $url = "https://github.com/$Repo/releases/download/$Version/$artifact"
}

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
$target = Join-Path $InstallDir "mcpcall.exe"
$tmpFile = Join-Path ([System.IO.Path]::GetTempPath()) ([System.IO.Path]::GetRandomFileName())

try {
    $headers = @{}
    if ($env:GITHUB_TOKEN) {
        $headers["Authorization"] = "Bearer $env:GITHUB_TOKEN"
    }
    Invoke-WebRequest -Uri $url -OutFile $tmpFile -Headers $headers
    Move-Item -Force -LiteralPath $tmpFile -Destination $target
} finally {
    if (Test-Path -LiteralPath $tmpFile) {
        Remove-Item -Force -LiteralPath $tmpFile
    }
}

if ($env:GITHUB_PATH) {
    Add-Content -Path $env:GITHUB_PATH -Value $InstallDir
} elseif (-not (($env:PATH -split [System.IO.Path]::PathSeparator) -contains $InstallDir)) {
    Write-Warning "Add $InstallDir to PATH to run mcpcall from a new shell."
}

& $target --version
Write-Host "installed $target"
