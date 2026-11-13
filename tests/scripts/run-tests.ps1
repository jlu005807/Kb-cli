param()

$ErrorActionPreference = "Stop"

Write-Host "Running cargo check --workspace"
cargo check --workspace
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "Running cargo test --workspace"
cargo test --workspace
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

if (Get-Command node -ErrorAction SilentlyContinue) {
    Write-Host "Running packaging regression tests"
    node --test tests/tools/packaging.test.js
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    Write-Host "Running doc-fixture validator regression test"
    node --test tests/tools/validate-doc-fixture-diff.test.js
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    Write-Host "Running Node-based doc-fixture diff (fallback)"
    node tests/tools/validate-doc-fixture-diff.js
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} else {
    Write-Host "node not found; skipping Node-based diff"
}

Write-Host "All tests completed successfully."
