$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '..\install.ps1')

function Assert-True([bool] $Condition, [string] $Message) {
    if (-not $Condition) { throw $Message }
}

$testDirectory = Join-Path $env:TEMP ("snap-install-test-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $testDirectory | Out-Null
try {
    $configPath = Join-Path $testDirectory 'mcp.json'
    $original = @{
        editor = 'keep this setting'
        mcpServers = @{
            other = @{ command = 'other-tool'; args = @('--keep') }
        }
    } | ConvertTo-Json -Depth 10
    Set-Content -LiteralPath $configPath -Value $original -Encoding UTF8

    Merge-SnapServer $configPath
    $first = Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json
    Assert-True ($first.editor -eq 'keep this setting') 'Unrelated top-level settings changed.'
    Assert-True ($first.mcpServers.other.command -eq 'other-tool') 'The existing MCP entry changed.'
    Assert-True ($first.mcpServers.snap.command -eq $venvPython) 'The Snap Python path is incorrect.'
    Assert-True ($first.mcpServers.snap.args[0] -eq $serverPath) 'The Snap server path is incorrect.'

    Merge-SnapServer $configPath
    $second = Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json
    Assert-True (@($second.mcpServers.PSObject.Properties | Where-Object Name -eq 'snap').Count -eq 1) 'Rerunning duplicated the Snap entry.'
    Assert-True ($second.mcpServers.other.command -eq 'other-tool') 'Rerunning changed the existing MCP entry.'

    $malformedPath = Join-Path $testDirectory 'malformed.json'
    $malformed = '{ invalid json'
    Set-Content -LiteralPath $malformedPath -Value $malformed -NoNewline -Encoding UTF8
    $threw = $false
    try { Merge-SnapServer $malformedPath } catch { $threw = $true }
    Assert-True $threw 'Malformed JSON was not rejected.'
    Assert-True ((Get-Content -LiteralPath $malformedPath -Raw) -eq $malformed) 'Malformed JSON was overwritten.'

    $wrongTypePath = Join-Path $testDirectory 'wrong-type.json'
    $wrongType = '{"mcpServers":[]}'
    Set-Content -LiteralPath $wrongTypePath -Value $wrongType -NoNewline -Encoding UTF8
    $threw = $false
    try { Merge-SnapServer $wrongTypePath } catch { $threw = $true }
    Assert-True $threw 'An invalid mcpServers value was not rejected.'
    Assert-True ((Get-Content -LiteralPath $wrongTypePath -Raw) -eq $wrongType) 'An invalid mcpServers value was overwritten.'

    Write-Host 'Windows install configuration checks passed.'
}
finally {
    Remove-Item -LiteralPath $testDirectory -Recurse -Force
}
