# Every check before a commit or a review (docs/review/README.md, section 4), on Windows.
# Run from the repository root:  powershell -ExecutionPolicy Bypass -File tools\check-all.ps1
# It builds and tests: only build outputs change (target, apps\desktop\dist); cargo fmt only checks.
# The browser checks need the preview server: they are run last, and the script starts and
# stops it itself.

# Native tools report failure through their exit codes (checked in Step), not as errors.
$ErrorActionPreference = "Continue"
$failed = @()
function Step($name, [scriptblock]$run) {
    Write-Host "`n=== $name" -ForegroundColor Cyan
    & $run
    if ($LASTEXITCODE -ne 0) { $script:failed += $name; Write-Host "FAILED: $name" -ForegroundColor Red }
}

Step "format"            { cargo fmt --all --check }
Step "lint"              { cargo clippy --workspace --all-targets -- -D warnings }
Step "lint with moving"  { cargo clippy --workspace --all-targets --features dc-desktop/moving -- -D warnings }
Step "tests"             { cargo test --workspace }
Step "removal tests"     { cargo test -p dc-quarantine --features destructive-actions }
Step "dependencies"      { cargo deny check }
Step "rules"             { cargo run -q -p dc-cli -- rules --check rules/junk/builtin.toml }
Step "desktop build"     { cargo build -p dc-desktop --features moving }

Push-Location apps\desktop
try {
    Step "page types"    { npm run -s check }
    Step "page tests"    { npx vitest run }
    Step "page build"    { npm run -s build }
    node -e "require('playwright')" 2>$null
    if ($LASTEXITCODE -ne 0) {
        Write-Host "`n=== browser checks: skipped (Playwright is not installed here; they run in the build container)" -ForegroundColor Yellow
        $LASTEXITCODE = 0
    } else {
        $preview = Start-Process -FilePath "npx.cmd" -ArgumentList "vite","preview","--port","4173" -PassThru -WindowStyle Hidden
        Start-Sleep -Seconds 3
        try { Step "browser checks" { npm run -s e2e } }
        finally {
            # Stop the preview server this script started (and the node process it runs in).
            Get-NetTCPConnection -LocalPort 4173 -State Listen -ErrorAction SilentlyContinue |
                ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }
            Stop-Process -Id $preview.Id -Force -ErrorAction SilentlyContinue
        }
    }
} finally { Pop-Location }

if ($failed.Count -eq 0) { Write-Host "`nALL CHECKS PASSED" -ForegroundColor Green; exit 0 }
Write-Host "`nFailed: $($failed -join ', ')" -ForegroundColor Red
exit 1
