$ErrorActionPreference = "Stop"

$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Push-Location $projectRoot

function Invoke-CargoStep {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,

        [Parameter(Mandatory = $true)]
        [string[]]$Arguments
    )

    Write-Host "`n==> $Name" -ForegroundColor Cyan
    $cargoArguments = $Arguments
    & cargo @cargoArguments

    if ($LASTEXITCODE -ne 0) {
        throw "Step '$Name' failed with exit code $LASTEXITCODE."
    }
}

try {
    Invoke-CargoStep -Name "Check formatting" -Arguments @("fmt", "--all", "--check")
    Invoke-CargoStep -Name "Run Clippy" -Arguments @(
        "clippy",
        "--workspace",
        "--all-targets",
        "--locked",
        "--",
        "-D",
        "warnings"
    )
    Invoke-CargoStep -Name "Run workspace tests" -Arguments @(
        "test",
        "--workspace",
        "--all-features",
        "--locked"
    )
    Invoke-CargoStep -Name "Build the application" -Arguments @(
        "build",
        "-p",
        "fastnote-app",
        "--locked"
    )

    Write-Host "`nAll automated checks passed." -ForegroundColor Green
}
finally {
    Pop-Location
}
