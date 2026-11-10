<#
.SYNOPSIS
  Build a signed yoru release and everything GitHub Releases needs for in-app updates.

.EXAMPLE
  ./scripts/release.ps1 -Version 0.4.0
  ./scripts/release.ps1 -Version 0.4.1 -Notes "Fix SFTP rename on Windows servers"
  ./scripts/release.ps1 -Version 0.4.1 -Publish     # also upload with GitHub CLI (gh)

What it does
  1. Checks the git tree is clean and the version is newer than the current one.
  2. Bumps the version in package.json, src-tauri/tauri.conf.json, src-tauri/Cargo.toml.
  3. Runs typecheck + frontend tests + Rust tests (skip with -SkipTests).
  4. Builds the installers, signed with the private updater key
     (default: %USERPROFILE%\.tauri\yoru-updater.key — never inside the repo).
  5. Writes release\v<version>\latest.json pointing at the GitHub release asset.
  6. Commits "Release <version>" and tags v<version>.
  7. Prints (or with -Publish runs) the commands to push and upload.
#>
param(
    [Parameter(Mandatory = $true)][string]$Version,
    [string]$Notes = "",
    [string]$Repo = "sundeiii/waga",
    [string]$KeyPath = "$env:USERPROFILE\.tauri\yoru-updater.key",
    [switch]$SkipTests,
    [switch]$Publish
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

function Step($msg) { Write-Host "`n==> $msg" -ForegroundColor Magenta }
function Fail($msg) { Write-Host "error: $msg" -ForegroundColor Red; exit 1 }

# ---- checks
if ($Version -notmatch '^\d+\.\d+\.\d+$') { Fail "Version must look like 1.2.3" }
if (-not (Test-Path $KeyPath)) { Fail "Signing key not found at $KeyPath. Without it no update can be signed." }
if (git status --porcelain) { Fail "Working tree has uncommitted changes. Commit or stash them first." }
$current = (Get-Content src-tauri/tauri.conf.json -Raw | ConvertFrom-Json).version
if ([version]$Version -le [version]$current) { Fail "Version $Version must be newer than the current $current" }
if (git tag --list "v$Version") { Fail "Tag v$Version already exists" }

# ---- bump
Step "Bumping $current -> $Version"
foreach ($f in @("package.json", "src-tauri/tauri.conf.json")) {
    $c = Get-Content $f -Raw
    $c = $c -replace ('"version":\s*"' + [regex]::Escape($current) + '"'), ('"version": "' + $Version + '"')
    [IO.File]::WriteAllText((Resolve-Path $f), $c)
}
$cargo = Get-Content src-tauri/Cargo.toml -Raw
$cargo = $cargo -replace ('(?m)^version = "' + [regex]::Escape($current) + '"'), ('version = "' + $Version + '"')
[IO.File]::WriteAllText((Resolve-Path src-tauri/Cargo.toml), $cargo)

try {
    # ---- tests
    if (-not $SkipTests) {
        Step "Typecheck + tests"
        npx tsc --noEmit -p . ; if ($LASTEXITCODE) { throw "typecheck failed" }
        npx vitest run ; if ($LASTEXITCODE) { throw "frontend tests failed" }
        Push-Location src-tauri
        cargo test --quiet ; $code = $LASTEXITCODE
        Pop-Location
        if ($code) { throw "Rust tests failed" }
    }

    # ---- build (signed)
    Step "Building signed installers"
    $env:TAURI_SIGNING_PRIVATE_KEY = $KeyPath
    $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""
    npm run tauri build ; if ($LASTEXITCODE) { throw "build failed" }
} catch {
    Write-Host "`nRolling back version bump..." -ForegroundColor Yellow
    git checkout -- package.json src-tauri/tauri.conf.json src-tauri/Cargo.toml src-tauri/Cargo.lock 2>$null
    Fail $_
} finally {
    Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY -ErrorAction SilentlyContinue
    Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD -ErrorAction SilentlyContinue
}

$bundle = "src-tauri/target/release/bundle"
$setup = "$bundle/nsis/Yoru_${Version}_x64-setup.exe"
$msi = "$bundle/msi/Yoru_${Version}_x64_en-US.msi"
$sig = "$setup.sig"
foreach ($p in @($setup, $sig, $msi)) { if (-not (Test-Path $p)) { Fail "Expected build output missing: $p" } }

# ---- release notes
if (-not $Notes) {
    $prev = git describe --tags --abbrev=0 2>$null
    $range = if ($prev) { "$prev..HEAD" } else { "HEAD" }
    $Notes = (git log $range --pretty=format:"- %s" --no-merges | Where-Object { $_ -notmatch '^- Release ' }) -join "`n"
    if (-not $Notes) { $Notes = "Maintenance release." }
}

# ---- latest.json
Step "Writing latest.json"
$out = "release/v$Version"
New-Item -ItemType Directory -Force $out | Out-Null
$assetName = Split-Path $setup -Leaf
$manifest = [ordered]@{
    version   = $Version
    notes     = $Notes
    pub_date  = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
    platforms = [ordered]@{
        "windows-x86_64" = [ordered]@{
            signature = (Get-Content $sig -Raw).Trim()
            url       = "https://github.com/$Repo/releases/download/v$Version/$assetName"
        }
    }
}
$manifest | ConvertTo-Json -Depth 5 | Set-Content "$out/latest.json" -Encoding utf8
Copy-Item $setup, $sig, $msi $out
$Notes | Set-Content "$out/NOTES.md" -Encoding utf8

# ---- commit + tag
Step "Committing and tagging v$Version"
git add package.json src-tauri/tauri.conf.json src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -q -m "Release $Version"
git tag "v$Version"

$files = "`"$out/$assetName`" `"$out/$assetName.sig`" `"$out/$(Split-Path $msi -Leaf)`" `"$out/latest.json`""
$ghCmd = "gh release create v$Version $files --repo $Repo --title `"yoru $Version`" --notes-file `"$out/NOTES.md`""

if ($Publish) {
    if (-not (Get-Command gh -ErrorAction SilentlyContinue)) { Fail "GitHub CLI (gh) not found. Install: winget install GitHub.cli" }
    Step "Pushing and publishing"
    git push origin HEAD --tags
    Invoke-Expression $ghCmd
    Write-Host "`nPublished. Installed copies will offer the update on their next check." -ForegroundColor Green
} else {
    Write-Host "`nRelease v$Version is ready in $out" -ForegroundColor Green
    Write-Host "To publish:"
    Write-Host "  git push origin HEAD --tags"
    Write-Host "  $ghCmd"
    Write-Host "(or upload those 4 files on github.com/$Repo/releases/new for tag v$Version)"
}
