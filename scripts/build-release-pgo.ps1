# Builds a profile-guided release of Gobbo. The profile is trained by the backend benchmark, which covers status, plan,
# import and deploy. `-Package` runs the normal release command afterwards, so the installers and the portable archive
# contain the PGO executable; without it, the script leaves target/pgo-release/release/gobbo.exe for inspection.
#
# Prerequisites: `rustup component add llvm-tools-preview` (provides llvm-profdata.exe). Pass -MergedProfile to reuse a
# profile from an earlier run, or let the script train a new one. The normal `bun run build` stays reproducible; use
# `bun run build:pgo` for the performance release.
param(
    [string]$ProfileDir = (Join-Path $PSScriptRoot '..\target\pgo\profiles'),
    [string]$BenchFilter = 'status|plan|import|deploy',
    [string]$MergedProfile = '',
    [switch]$Package
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$src = Join-Path $root 'src-tauri'
$sysroot = (& rustc --print sysroot).Trim()
$hostLine = (& rustc -vV | Select-String '^host:').Line
$hostTriple = $hostLine.Substring($hostLine.IndexOf(':') + 1).Trim()
$profdata = Join-Path $sysroot "lib\rustlib\$hostTriple\bin\llvm-profdata.exe"
if (-not (Test-Path $profdata)) {
    throw "llvm-profdata was not found at $profdata. Run: rustup component add llvm-tools-preview"
}
New-Item -ItemType Directory -Path $ProfileDir -Force | Out-Null
Get-ChildItem $ProfileDir -Filter '*.profraw' -ErrorAction SilentlyContinue | Remove-Item -Force

$instrumented = Join-Path $src 'target\pgo-instrumented'
$output = Join-Path $src 'target\pgo-release'
Push-Location $src
try {
    if ($MergedProfile) {
        $merged = (Resolve-Path $MergedProfile).Path
        Write-Output "Reusing the profile at $merged."
    } else {
        Write-Output 'Building the instrumented benchmark...'
        $env:RUSTFLAGS = "-Cprofile-generate=$ProfileDir"
        $env:CARGO_TARGET_DIR = $instrumented
        cargo bench --profile release-bench --features bench --bench backend --no-run
        if ($LASTEXITCODE -ne 0) { throw 'the instrumented benchmark build failed' }

        $bench = Get-ChildItem (Join-Path $instrumented 'release-bench\deps') -Filter 'backend-*.exe' |
            Sort-Object LastWriteTime | Select-Object -Last 1
        if (-not $bench) { throw 'the backend benchmark executable was not found' }

        Write-Output "Training the profile with $BenchFilter..."
        & $bench.FullName --bench $BenchFilter --warm-up-time 0.2 --measurement-time 0.6 --sample-size 10 --noplot
        if ($LASTEXITCODE -ne 0) { throw 'the PGO training run failed' }

        $merged = Join-Path (Split-Path $ProfileDir) 'merged.profdata'
        & $profdata merge -o $merged $ProfileDir
        if ($LASTEXITCODE -ne 0) { throw 'llvm-profdata merge failed' }
    }

    Write-Output 'Building the profile-guided release...'
    $env:RUSTFLAGS = "-Cprofile-use=$merged -Cllvm-args=-pgo-warn-missing-function"
    if ($Package) {
        # The normal release command also builds the hook, the frontend and the installers. The hook script removes
        # the profile flags from its own cargo calls, so only the app is profile-guided.
        Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue
        Push-Location $root
        bun run build
        if ($LASTEXITCODE -ne 0) { Pop-Location; throw 'the packaged PGO release failed' }
        Pop-Location
        Write-Output "PGO release and packages are in $(Join-Path $root 'dist-release')."
    } else {
        $env:CARGO_TARGET_DIR = $output
        cargo build --release
        if ($LASTEXITCODE -ne 0) { throw 'the profile-guided release build failed' }
        $exe = Join-Path $output 'release\gobbo.exe'
        $size = (Get-Item $exe).Length
        Write-Output "PGO release: $exe ($size bytes)"
        Write-Output 'Build the overlay hook with `bun run build:overlay-hook` before packaging.'
    }
} finally {
    Remove-Item Env:\RUSTFLAGS -ErrorAction SilentlyContinue
    Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue
    Pop-Location
}
