param(
  [Parameter(Mandatory = $true)]
  [ValidateScript({ Test-Path $_ -PathType Leaf })]
  [string]$HostExecutable,
  [string]$ManifestDirectory = (Join-Path $env:LOCALAPPDATA 'PC Snapshot\BrowserCompanion')
)

$ErrorActionPreference = 'Stop'
$hostName = 'app.pcsnapshot.companion'
$extensionId = 'chfbdgfhlkbocpeofdjkincopepifnlj'

New-Item -ItemType Directory -Force -Path $ManifestDirectory | Out-Null
$manifestPath = Join-Path $ManifestDirectory 'chromium-host.json'
$manifest = @{
  name = $hostName
  description = 'PC Snapshot Browser Companion'
  path = [System.IO.Path]::GetFullPath($HostExecutable)
  type = 'stdio'
  allowed_origins = @("chrome-extension://$extensionId/")
} | ConvertTo-Json -Depth 4
[System.IO.File]::WriteAllText($manifestPath, $manifest + [Environment]::NewLine, [System.Text.UTF8Encoding]::new($false))

$targets = @(
  @{ Label = 'Chrome'; Root = 'HKCU:\Software\Google\Chrome\NativeMessagingHosts' },
  @{ Label = 'Edge'; Root = 'HKCU:\Software\Microsoft\Edge\NativeMessagingHosts' },
  @{ Label = 'Opera'; Root = 'HKCU:\Software\Opera Software\Opera Stable\NativeMessagingHosts' },
  @{ Label = 'Opera GX'; Root = 'HKCU:\Software\Opera Software\Opera GX Stable\NativeMessagingHosts' },
  @{ Label = 'Brave'; Root = 'HKCU:\Software\BraveSoftware\Brave-Browser\NativeMessagingHosts' }
)

$registered = foreach ($target in $targets) {
  $browserKey = Split-Path -Path $target.Root -Parent
  if (-not (Test-Path $browserKey)) {
    Write-Output "Skipped $($target.Label): browser registry key not found"
    continue
  }

  $hostKey = Join-Path $target.Root $hostName
  New-Item -Force -Path $hostKey | Out-Null
  Set-Item -Path $hostKey -Value $manifestPath
  $target.Label
}

Write-Output "Registered $hostName for $($registered -join ', '): $manifestPath"
