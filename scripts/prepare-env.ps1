# Run with dot-sourcing so the environment stays in the current PowerShell session:
#   . .\scripts\prepare-env.ps1

[CmdletBinding()]
param(
    # Optional path to an OpenSSL installation to use instead of vendored curl/OpenSSL.
    [string]$OpenSslDir,
    [switch]$EmitDirenv
)

$ErrorActionPreference = "Stop"
$initialEnvironment = @{}

Get-ChildItem Env: | ForEach-Object {
    $initialEnvironment[$_.Name] = $_.Value
}

function Import-CmdEnvironment
{
    param(
        [Parameter(Mandatory = $true)]
        [string]$BatchFile,
        [string]$Arguments = ""
    )

    $commandLine = "`"$BatchFile`" $Arguments >nul && set"
    $lines = & cmd.exe /s /c $commandLine
    if ($LASTEXITCODE -ne 0)
    {
        throw "Failed to import environment from command: $commandLine"
    }

    foreach ($line in $lines)
    {
        if ([string]::IsNullOrWhiteSpace($line) -or $line -notmatch "=")
        {
            continue
        }
        $parts = $line -split "=", 2
        if ($parts.Count -ne 2)
        {
            continue
        }
        [Environment]::SetEnvironmentVariable($parts[0], $parts[1], "Process")
    }
}

function Get-VsDevCmdPath
{
    $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    if (-not (Test-Path $vswhere))
    {
        throw "vswhere.exe not found at '$vswhere'"
    }

    $installationPath = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($installationPath))
    {
        throw "Could not locate a Visual Studio installation with C++ tools"
    }

    $vsDevCmd = Join-Path $installationPath "Common7\Tools\VsDevCmd.bat"
    if (-not (Test-Path $vsDevCmd))
    {
        throw "VsDevCmd.bat not found at '$vsDevCmd'"
    }

    return $vsDevCmd
}

function Remove-PathEntriesMatching
{
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$Patterns
    )

    $pathEntries = (${env:PATH} -split ";").Where({ $_ -and $_.Trim() -ne "" })
    $filtered = foreach ($entry in $pathEntries)
    {
        $normalized = $entry.Replace("\", "/").ToLowerInvariant()
        $drop = $false
        foreach ($pattern in $Patterns)
        {
            if ($normalized -like $pattern.ToLowerInvariant())
            {
                $drop = $true
                break
            }
        }
        if (-not $drop)
        {
            $entry
        }
    }
    $env:PATH = ($filtered -join ";")
}

function Resolve-OpenSslDir
{
    param(
        [string]$PreferredPath
    )

    if (-not [string]::IsNullOrWhiteSpace($PreferredPath) -and (Test-Path $PreferredPath))
    {
        return $PreferredPath
    }

    if (-not [string]::IsNullOrWhiteSpace($env:OPENSSL_DIR) -and (Test-Path $env:OPENSSL_DIR))
    {
        return $env:OPENSSL_DIR
    }

    $scoop = Get-Command scoop -ErrorAction SilentlyContinue
    if ($null -ne $scoop)
    {
        try
        {
            $prefix = (& scoop prefix openssl 2>$null | Select-Object -First 1).Trim()
            if (-not [string]::IsNullOrWhiteSpace($prefix) -and (Test-Path $prefix))
            {
                return $prefix
            }
        }
        catch
        {
            # Ignore auto-detection failures and fall back to vendored dependencies.
        }
    }

    return $null
}

function ConvertTo-ShSingleQuoted
{
    param(
        [AllowEmptyString()]
        [string]$Value
    )

    $singleQuoteEscape = "'`"'`"'"
    return "'" + $Value.Replace("'", $singleQuoteEscape) + "'"
}

function Write-DirenvExports
{
    $currentEnvironment = @{}
    Get-ChildItem Env: | ForEach-Object {
        $currentEnvironment[$_.Name] = $_.Value
    }

    foreach ($name in ($currentEnvironment.Keys | Sort-Object))
    {
        $currentValue = $currentEnvironment[$name]
        if (-not $initialEnvironment.ContainsKey($name) -or $initialEnvironment[$name] -ne $currentValue)
        {
            if ($name -notmatch '^[A-Za-z_][A-Za-z0-9_]*$')
            {
                Write-Warning "Skipping '$name' because it is not a valid POSIX shell variable name"
                continue
            }
            Write-Output "export $name=$(ConvertTo-ShSingleQuoted $currentValue)"
        }
    }
}

# 1. Load the Visual Studio Developer Environment (MSVC)
$vsDevCmd = Get-VsDevCmdPath
Import-CmdEnvironment -BatchFile $vsDevCmd -Arguments "-arch=x64 -host_arch=x64"

# 2. Safety Net: Scrub MSYS2/MinGW from the PATH to prevent GNU linker conflicts
Remove-PathEntriesMatching @(
    "*/msys2/current/mingw64/bin",
    "*/msys2/current/ucrt64/bin",
    "*/msys64/mingw64/bin",
    "*/msys2/*/mingw64/bin"
)

# 3. Explicitly set Rust's C/C++ compilers to Microsoft's cl.exe
$env:CC = "cl.exe"
$env:CXX = "cl.exe"
$env:AR = "lib.exe"

$env:CC_x86_64_pc_windows_msvc = "cl.exe"
$env:CXX_x86_64_pc_windows_msvc = "cl.exe"
$env:AR_x86_64_pc_windows_msvc = "lib.exe"

# 4. Prefer a system OpenSSL when one is available.
$resolvedOpenSslDir = Resolve-OpenSslDir -PreferredPath $OpenSslDir
if (-not [string]::IsNullOrWhiteSpace($resolvedOpenSslDir))
{
    $env:OPENSSL_DIR = $resolvedOpenSslDir
    $env:OPENSSL_NO_VENDOR = 1
    $env:CURL_NO_VENDOR = 1
}

if ($EmitDirenv)
{
    Write-DirenvExports
    return
}

Write-Host "Pure MSVC environment ready for Zellij!" -ForegroundColor Green
Write-Host "Compiler (CC/CXX): $env:CC"
if (-not [string]::IsNullOrWhiteSpace($resolvedOpenSslDir))
{
    Write-Host "OPENSSL_DIR: $env:OPENSSL_DIR"
    Write-Host "Vendoring: Disabled (OPENSSL_NO_VENDOR=1)"
}
else
{
    Write-Host "OPENSSL_DIR: not set (vendored curl/OpenSSL remains available when explicitly enabled)"
}
