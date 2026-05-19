<#
.SYNOPSIS
    Build the docx-auto-template-engine and publish a self-contained runtime.

.DESCRIPTION
    Wraps `dotnet publish` with the standard flags. Run from the repo root.
    Daily skill users do NOT need this — only run when you change C# source
    or need a non-default RID build.

.PARAMETER Rid
    Runtime identifier. Default: win-x64. Examples: linux-x64, osx-arm64.

.PARAMETER Configuration
    Build configuration. Default: Release.

.PARAMETER OutputDir
    Output directory. Default: engine/runtime (matches the path SKILL.md expects).

.EXAMPLE
    .\build.ps1
    # Builds win-x64 Release into engine/runtime/

.EXAMPLE
    .\build.ps1 -Rid linux-x64
    # Builds linux-x64 Release into engine/runtime/
#>
[CmdletBinding()]
param(
    [string]$Rid = "win-x64",
    [string]$Configuration = "Release",
    [string]$OutputDir = "engine/runtime"
)

$ErrorActionPreference = "Stop"

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectPath = Join-Path $scriptRoot "engine/src/docx-auto-template-engine.csproj"

if (-not (Test-Path $projectPath)) {
    Write-Error "Project file not found at $projectPath. Run this script from the repository root."
    exit 1
}

if (-not (Get-Command dotnet -ErrorAction SilentlyContinue)) {
    Write-Error ".NET SDK not found. Install .NET SDK 8.0+ from https://dotnet.microsoft.com/download"
    exit 1
}

Write-Host "Building docx-auto-template-engine"
Write-Host "  RID:           $Rid"
Write-Host "  Configuration: $Configuration"
Write-Host "  Output:        $OutputDir"
Write-Host ""

$absOutput = Join-Path $scriptRoot $OutputDir

# Single-file + compression keeps the engine zero-install (self-contained:
# no Office/WPS/.NET required on the user's machine) while roughly halving
# on-disk size. IL trimming is deliberately NOT enabled: DocumentFormat.OpenXml
# is reflection-heavy and trimming risks silent runtime breakage.
dotnet publish $projectPath `
    -c $Configuration `
    -r $Rid `
    --self-contained true `
    -p:PublishSingleFile=true `
    -p:EnableCompressionInSingleFile=true `
    -p:DebugType=none `
    -o $absOutput

if ($LASTEXITCODE -ne 0) {
    Write-Error "dotnet publish failed with exit code $LASTEXITCODE"
    exit $LASTEXITCODE
}

Write-Host ""
Write-Host "Build complete. Engine published to $absOutput"
