param(
    [string]$OutputPath = (Join-Path (Split-Path -Parent $PSScriptRoot) 'THIRD_PARTY_NOTICES.md')
)

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repositoryRoot
try {
    $metadata = cargo metadata --format-version 1 --locked | ConvertFrom-Json
    $workspaceIds = @($metadata.workspace_members)
    $packages = @($metadata.packages | Where-Object { $workspaceIds -notcontains $_.id } | Sort-Object name, version)

    $lines = [Collections.Generic.List[string]]::new()
    $lines.Add('# Third-Party Notices')
    $lines.Add('')
    $lines.Add('This inventory is generated from the locked Cargo graph by `scripts/generate_dependency_notices.ps1`. License identifiers are package metadata, not a replacement for the complete license texts distributed by each dependency.')
    $lines.Add('')
    $lines.Add('## Bundled font')
    $lines.Add('')
    $lines.Add('- **Press Start 2P Regular** — Copyright 2012 The Press Start 2P Project Authors (cody@zone38.net), with Reserved Font Name "Press Start 2P". Licensed under the SIL Open Font License 1.1; see `assets/fonts/OFL.txt`.')
    $lines.Add('')
    $lines.Add('## Visual assets')
    $lines.Add('')
    $lines.Add('The application icon and SVG controls in `assets/` are PixelBuddy project assets. The Milestone 4 reference audit found no additional third-party visual assets. Add a notice and the applicable license before introducing any third-party asset.')
    $lines.Add('')
    $lines.Add('## Locked Rust dependency inventory')
    $lines.Add('')
    $lines.Add('| Package | Version | License | Upstream |')
    $lines.Add('|---|---:|---|---|')
    foreach ($package in $packages) {
        $license = if ([string]::IsNullOrWhiteSpace($package.license)) { 'Not specified' } else { $package.license }
        $upstream = if (-not [string]::IsNullOrWhiteSpace($package.repository)) { $package.repository } elseif (-not [string]::IsNullOrWhiteSpace($package.source)) { $package.source } else { '' }
        $license = $license.Replace('|', '\|')
        $upstream = $upstream.Replace('|', '%7C')
        $lines.Add("| $($package.name) | $($package.version) | $license | $upstream |")
    }
    $lines.Add('')
    $lines.Add("Generated from ``Cargo.lock``. CI independently enforces the allowed SPDX set with ``cargo-deny``.")

    [IO.File]::WriteAllLines($OutputPath, $lines, [Text.UTF8Encoding]::new($false))
}
finally {
    Pop-Location
}
