# Install or update Snap on Windows.
# Run from PowerShell: .\install.ps1
[CmdletBinding()]
param(
    [switch] $SkipAppInstall
)

$ErrorActionPreference = 'Stop'
$repo = 'adjective-rob/snap'
$installRoot = Join-Path $env:LOCALAPPDATA 'snap-annotate'
$script:releaseTag = $null
$mcpDirectory = Join-Path $installRoot 'mcp-server'
$serverPath = Join-Path $mcpDirectory 'server.py'
$venvDirectory = Join-Path $mcpDirectory '.venv'
$venvPython = Join-Path $venvDirectory 'Scripts\python.exe'

function Stop-Install([string] $Message) {
    throw $Message
}

function Invoke-Checked([string] $FilePath, [string[]] $Arguments) {
    & $FilePath @Arguments
    if ($LASTEXITCODE -ne 0) {
        Stop-Install "Command failed with exit code ${LASTEXITCODE}: $FilePath $($Arguments -join ' ')"
    }
}

function Merge-SnapServer([string] $ConfigPath) {
    $configDirectory = Split-Path -Parent $ConfigPath
    if (-not (Test-Path -LiteralPath $configDirectory -PathType Container)) {
        Write-Host "Skipping $ConfigPath (client not installed)."
        return
    }

    $config = [pscustomobject]@{}
    if (Test-Path -LiteralPath $ConfigPath -PathType Leaf) {
        try {
            $config = Get-Content -LiteralPath $ConfigPath -Raw | ConvertFrom-Json
        }
        catch {
            Stop-Install "Cannot read valid JSON from '$ConfigPath'. Fix that file and rerun; it was left unchanged. $($_.Exception.Message)"
        }
        if ($null -eq $config -or $config -is [array] -or $config -isnot [pscustomobject]) {
            Stop-Install "Expected a JSON object in '$ConfigPath'. The file was left unchanged."
        }
    }

    $serversProperty = $config.PSObject.Properties['mcpServers']
    if ($null -eq $serversProperty) {
        $servers = [pscustomobject]@{}
        $config | Add-Member -NotePropertyName 'mcpServers' -NotePropertyValue $servers
    }
    else {
        $servers = $serversProperty.Value
        if ($null -eq $servers -or $servers -is [array] -or $servers -isnot [pscustomobject]) {
            Stop-Install "Expected 'mcpServers' to be a JSON object in '$ConfigPath'. The file was left unchanged."
        }
    }

    $snapServer = [pscustomobject]@{
        command = $venvPython
        args = @($serverPath)
    }
    if ($null -eq $servers.PSObject.Properties['snap']) {
        $servers | Add-Member -NotePropertyName 'snap' -NotePropertyValue $snapServer
    }
    else {
        $servers.PSObject.Properties['snap'].Value = $snapServer
    }

    $json = ConvertTo-Json -InputObject $config -Depth 100
    $temporaryPath = "$ConfigPath.tmp"
    try {
        [System.IO.File]::WriteAllText($temporaryPath, $json + [Environment]::NewLine, [System.Text.UTF8Encoding]::new($false))
        Move-Item -LiteralPath $temporaryPath -Destination $ConfigPath -Force
    }
    finally {
        Remove-Item -LiteralPath $temporaryPath -Force -ErrorAction SilentlyContinue
    }
    Write-Host "Configured Snap MCP server in $ConfigPath"
}

function Install-LatestApp {
    $releaseUrl = "https://api.github.com/repos/$repo/releases/latest"
    try {
        $release = Invoke-RestMethod -Uri $releaseUrl -Headers @{ 'User-Agent' = 'Snap-Windows-Installer' }
    }
    catch {
        Stop-Install "No published Snap release could be found at $releaseUrl. The Windows installer is published with tagged releases. $($_.Exception.Message)"
    }
    $script:releaseTag = $release.tag_name

    $assets = @($release.assets | Where-Object { $_.name -match '(?i)^snap.*x64-setup\.exe$' })
    if ($assets.Count -ne 1) {
        Stop-Install "Expected one x64 Snap NSIS installer in release '$($release.tag_name)', found $($assets.Count). No installer was run."
    }

    $temporaryDirectory = Join-Path ([System.IO.Path]::GetTempPath()) ([guid]::NewGuid().ToString('N'))
    $installerPath = Join-Path $temporaryDirectory $assets[0].name
    New-Item -ItemType Directory -Path $temporaryDirectory | Out-Null
    try {
        Invoke-WebRequest -Uri $assets[0].browser_download_url -OutFile $installerPath
        Write-Host "Running Snap installer from release $($release.tag_name). Complete its installer prompts to continue."
        $process = Start-Process -FilePath $installerPath -Wait -PassThru
        if ($process.ExitCode -ne 0) {
            Stop-Install "Snap installer exited with code $($process.ExitCode)."
        }
    }
    finally {
        Remove-Item -LiteralPath $temporaryDirectory -Recurse -Force -ErrorAction SilentlyContinue
    }
}

function Get-SnapExecutable {
    $uninstallKeys = @(
        'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
        'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
        'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*'
    )
    foreach ($key in $uninstallKeys) {
        $entry = Get-ItemProperty -Path $key -ErrorAction SilentlyContinue |
            Where-Object { $_.DisplayName -match '^Snap(?:\s|$)' -and $_.InstallLocation } |
            Select-Object -First 1
        if ($null -ne $entry) {
            $candidate = Join-Path $entry.InstallLocation 'snap.exe'
            if (Test-Path -LiteralPath $candidate) { return $candidate }
        }
    }
    foreach ($candidate in @(
        (Join-Path $env:LOCALAPPDATA 'Programs\snap\snap.exe'),
        (Join-Path $env:LOCALAPPDATA 'Programs\Snap\snap.exe'),
        (Join-Path $env:LOCALAPPDATA 'snap\snap.exe'),
        (Join-Path $env:ProgramFiles 'snap\snap.exe'),
        (Join-Path $env:ProgramFiles 'Snap\snap.exe')
    )) {
        if (Test-Path -LiteralPath $candidate) { return $candidate }
    }
    return $null
}

function Start-SnapInstall {
    $git = Get-Command git -ErrorAction SilentlyContinue
    if ($null -eq $git) { Stop-Install 'Git is required to install or update Snap.' }
    $uv = Get-Command uv -ErrorAction SilentlyContinue
    $python = Get-Command python -ErrorAction SilentlyContinue
    if ($null -eq $uv) {
        if ($null -eq $python) { Stop-Install 'Python 3.11 or newer is required. Install uv or Python, then rerun this script.' }
        $version = & $python.Source -c 'import sys; print(f"{sys.version_info.major}.{sys.version_info.minor}")'
        if ($LASTEXITCODE -ne 0 -or [version]$version -lt [version]'3.11') {
            Stop-Install 'Python 3.11 or newer is required. Install uv or Python, then rerun this script.'
        }
    }

    if (Test-Path -LiteralPath (Join-Path $installRoot '.git')) {
        $changes = git -C $installRoot status --porcelain
        if ($LASTEXITCODE -ne 0) { Stop-Install "Could not inspect existing checkout at '$installRoot'." }
        if ($changes) { Stop-Install "Existing checkout '$installRoot' has local changes. Commit or stash them before updating; no files were changed." }
    }
    elseif (Test-Path -LiteralPath $installRoot) {
        Stop-Install "'$installRoot' exists but is not a Git checkout. Move it aside before installing Snap there."
    }

    if (-not $SkipAppInstall) {
        Install-LatestApp
    }
    $appPath = Get-SnapExecutable
    if (-not $appPath) { Stop-Install 'Could not find snap.exe after installation. Check the installer location and rerun this script.' }

    if (Test-Path -LiteralPath (Join-Path $installRoot '.git')) {
        if ($script:releaseTag) {
            Invoke-Checked 'git' @('-C', $installRoot, 'fetch', '--depth', '1', 'origin', "refs/tags/$script:releaseTag")
            Invoke-Checked 'git' @('-C', $installRoot, 'checkout', '--detach', 'FETCH_HEAD')
        }
        else {
            Invoke-Checked 'git' @('-C', $installRoot, 'pull', '--ff-only')
        }
    }
    else {
        if (Test-Path -LiteralPath $installRoot) {
            Stop-Install "'$installRoot' exists but is not a Git checkout. Move it aside before installing Snap there."
        }
        if ($script:releaseTag) {
            Invoke-Checked 'git' @('clone', '--depth', '1', '--branch', $script:releaseTag, "https://github.com/$repo.git", $installRoot)
        }
        else {
            Invoke-Checked 'git' @('clone', "https://github.com/$repo.git", $installRoot)
        }
    }

    if ($null -ne $uv) {
        Invoke-Checked $uv.Source @('venv', '--allow-existing', $venvDirectory)
        Invoke-Checked $uv.Source @('pip', 'install', '--python', $venvPython, '-e', $mcpDirectory)
    }
    else {
        Invoke-Checked $python.Source @('-m', 'venv', $venvDirectory)
        Invoke-Checked $venvPython @('-m', 'pip', 'install', '-e', $mcpDirectory)
    }

    $claude = Get-Command claude -ErrorAction SilentlyContinue
    if ($null -ne $claude) {
        Invoke-Checked $claude.Source @('mcp', 'add', '--scope', 'user', '--transport', 'stdio', 'snap', '--', $venvPython, $serverPath)
    }
    Merge-SnapServer (Join-Path $env:APPDATA 'Claude\claude_desktop_config.json')
    Merge-SnapServer (Join-Path $env:USERPROFILE '.cursor\mcp.json')
    Merge-SnapServer (Join-Path $env:USERPROFILE '.codeium\windsurf\mcp_config.json')

    $startupDirectory = [Environment]::GetFolderPath('Startup')
    $shortcutPath = Join-Path $startupDirectory 'Snap.lnk'
    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut($shortcutPath)
    $shortcut.TargetPath = $appPath
    $shortcut.WorkingDirectory = Split-Path -Parent $appPath
    $shortcut.Save()
    Write-Host "Snap will start when you sign in ($shortcutPath)."

    Write-Host 'Snap setup is complete. Press Ctrl+Shift+S to capture an annotation.'
}

# Dot-sourcing exposes helpers to the Windows test script without starting an install.
if ($MyInvocation.InvocationName -ne '.') {
    Start-SnapInstall
}
