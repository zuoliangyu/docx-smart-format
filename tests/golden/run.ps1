<#
.SYNOPSIS
    Golden behavior harness for docx-smart-format (architecture-rewrite safety net, phase R0).

.DESCRIPTION
    Drives the engine through render -> analyze -> apply using the frozen
    fixtures in tests/golden/fixtures, normalizes every produced .docx / .json
    (unzip, canonicalize XML, scrub volatile data: timestamps, rsids, GUIDs,
    absolute paths), and either records or verifies a snapshot.

    Volatile-data scrubbing is what makes the snapshot stable across runs of the
    same engine, so a mismatch means a real behavior change, not zip noise.

.PARAMETER Mode
    record  -> (re)write tests/golden/snapshots from current engine output.
    verify  -> regenerate and diff against the recorded snapshots. Default.

.EXAMPLE
    pwsh tests/golden/run.ps1 -Mode record
    pwsh tests/golden/run.ps1 -Mode verify
#>
[CmdletBinding()]
param(
    [ValidateSet('record', 'verify')]
    [string]$Mode = 'verify'
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression.FileSystem

$here   = Split-Path -Parent $MyInvocation.MyCommand.Path
$root   = (Resolve-Path (Join-Path $here '..\..')).Path
$engine = Join-Path $root 'engine\runtime\docx-auto-template-engine.exe'
$fix    = Join-Path $here 'fixtures'
$snap   = Join-Path $here 'snapshots'
$work   = Join-Path $here '.work'

if (-not (Test-Path $engine)) {
    throw "Reference engine not found at $engine. Build it first: dotnet build engine/src/docx-auto-template-engine.csproj -c Release -o engine/runtime"
}
if (Test-Path $work) { Remove-Item $work -Recurse -Force }
New-Item -ItemType Directory -Path $work | Out-Null
if ($Mode -eq 'record' -and -not (Test-Path $snap)) { New-Item -ItemType Directory -Path $snap | Out-Null }

# --- normalization helpers -------------------------------------------------

$rootRegex = [Regex]::Escape($root)

function Scrub([string]$text) {
    if ($null -eq $text) { return '' }
    $t = $text -replace "`r`n", "`n"
    # absolute repo path in either slash style
    $t = [Regex]::Replace($t, $rootRegex, '<ROOT>', 'IgnoreCase')
    $t = [Regex]::Replace($t, ($rootRegex -replace '\\\\', '/'), '<ROOT>', 'IgnoreCase')
    # W3CDTF / ISO timestamps
    $t = [Regex]::Replace($t, '\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?Z?', '<TS>')
    # OOXML revision save ids (rsid*, w15:... paraId/textId)
    $t = [Regex]::Replace($t, '(rsid[A-Za-z]*|paraId|textId)="[0-9A-Fa-f]+"', '$1="<RSID>"')
    # GUIDs
    $t = [Regex]::Replace($t, '[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}', '<GUID>')
    return $t
}

function Format-Xml([byte[]]$bytes) {
    $text = [System.Text.Encoding]::UTF8.GetString($bytes)
    if ($text.Length -gt 0 -and $text[0] -eq [char]0xFEFF) { $text = $text.Substring(1) }
    $doc = New-Object System.Xml.XmlDocument
    $doc.PreserveWhitespace = $false
    $doc.LoadXml($text)
    $sw  = New-Object System.IO.StringWriter
    $set = New-Object System.Xml.XmlWriterSettings
    $set.Indent = $true
    $set.IndentChars = '  '
    $set.OmitXmlDeclaration = $false
    $xw = [System.Xml.XmlWriter]::Create($sw, $set)
    $doc.Save($xw); $xw.Flush(); $sw.ToString()
}

function Canonicalize-Rids([string]$text) {
    # OpenXML emits random relationship ids (R + 16 hex) each run. Remap them by
    # order of first appearance so identity/cross-reference is preserved but the
    # literal value is deterministic. Run on the whole artifact, not per-part.
    # OPC package core-properties part gets a random 32-hex .psmdcp name.
    $text = [Regex]::Replace($text, '[0-9A-Fa-f]{32}\.psmdcp', '<PKGCORE>.psmdcp')
    $map = @{}
    $ev = [System.Text.RegularExpressions.MatchEvaluator] {
        param($m)
        $k = $m.Value
        if (-not $map.ContainsKey($k)) { $map[$k] = 'R__' + ($map.Count + 1) }
        $map[$k]
    }
    [Regex]::Replace($text, 'R[0-9A-Fa-f]{16}', $ev)
}

function Sha([byte[]]$bytes) {
    $h = [System.Security.Cryptography.SHA256]::Create().ComputeHash($bytes)
    -join ($h | ForEach-Object { $_.ToString('x2') })
}

function Normalize-Docx([string]$path) {
    $sb = New-Object System.Text.StringBuilder
    $zip = [System.IO.Compression.ZipFile]::OpenRead($path)
    try {
        foreach ($e in ($zip.Entries | Sort-Object FullName)) {
            $ms = New-Object System.IO.MemoryStream
            $es = $e.Open(); $es.CopyTo($ms); $es.Close()
            $bytes = $ms.ToArray()
            [void]$sb.AppendLine("### ENTRY $($e.FullName)")
            $ext = [System.IO.Path]::GetExtension($e.FullName).ToLowerInvariant()
            if ($ext -in '.xml', '.rels') {
                [void]$sb.AppendLine((Scrub (Format-Xml $bytes)))
            }
            else {
                [void]$sb.AppendLine("<binary len=$($bytes.Length) sha=$(Sha $bytes)>")
            }
        }
    }
    finally { $zip.Dispose() }
    $sb.ToString()
}

function Normalize-Artifact([string]$path) {
    if (-not (Test-Path $path)) { return '<MISSING ARTIFACT>' }
    $ext = [System.IO.Path]::GetExtension($path).ToLowerInvariant()
    if ($ext -eq '.docx') { return Normalize-Docx $path }
    return Scrub ([System.IO.File]::ReadAllText($path))
}

# --- run one engine case ---------------------------------------------------

function Invoke-Case([string]$name, [string[]]$engineArgs, [string]$artifact) {
    Write-Host "[$Mode] case: $name"
    $so = Join-Path $work "$name.stdout"
    $se = Join-Path $work "$name.stderr"
    $p = Start-Process -FilePath $engine -ArgumentList $engineArgs -NoNewWindow -Wait -PassThru `
        -RedirectStandardOutput $so -RedirectStandardError $se
    $report = New-Object System.Text.StringBuilder
    [void]$report.AppendLine("=== CASE $name")
    [void]$report.AppendLine("exit $($p.ExitCode)")
    [void]$report.AppendLine("--- stderr")
    [void]$report.AppendLine((Scrub ([System.IO.File]::ReadAllText($se))))
    [void]$report.AppendLine("--- stdout")
    [void]$report.AppendLine((Scrub ([System.IO.File]::ReadAllText($so))))
    [void]$report.AppendLine("--- artifact $([System.IO.Path]::GetFileName($artifact))")
    [void]$report.AppendLine((Normalize-Artifact $artifact))
    $norm = Canonicalize-Rids ($report.ToString() -replace "`r`n", "`n")

    $actualFile = Join-Path $work "$name.norm.txt"
    [System.IO.File]::WriteAllText($actualFile, $norm, (New-Object System.Text.UTF8Encoding($false)))
    $snapFile = Join-Path $snap "$name.norm.txt"

    if ($Mode -eq 'record') {
        Copy-Item $actualFile $snapFile -Force
        Write-Host "  recorded -> snapshots/$name.norm.txt ($($norm.Length) chars)"
        return $true
    }

    if (-not (Test-Path $snapFile)) {
        Write-Host "  MISSING SNAPSHOT snapshots/$name.norm.txt (run -Mode record)" -ForegroundColor Red
        return $false
    }
    $expected = [System.IO.File]::ReadAllText($snapFile) -replace "`r`n", "`n"
    if ($expected -eq $norm) {
        Write-Host "  OK (matches snapshot)" -ForegroundColor Green
        return $true
    }
    Write-Host "  DRIFT vs snapshots/$name.norm.txt" -ForegroundColor Red
    $exp = $expected -split "`n"; $act = $norm -split "`n"
    for ($i = 0; $i -lt [Math]::Max($exp.Count, $act.Count); $i++) {
        $a = if ($i -lt $exp.Count) { $exp[$i] } else { '<eof>' }
        $b = if ($i -lt $act.Count) { $act[$i] } else { '<eof>' }
        if ($a -ne $b) {
            Write-Host ("  first diff at line {0}:" -f ($i + 1)) -ForegroundColor Yellow
            Write-Host ("    snapshot: {0}" -f $a)
            Write-Host ("    current : {0}" -f $b)
            break
        }
    }
    return $false
}

# --- pipeline (order matters: analyze/apply consume render output) ----------

$basic   = Join-Path $work 'basic.docx'
$chem    = Join-Path $work 'chem.docx'
$applied = Join-Path $work 'applied.docx'
$results = @()

$results += Invoke-Case 'render-basic' `
    @('render', '--spec', (Join-Path $fix 'sample-render-spec.json'), '--output', $basic) $basic
$results += Invoke-Case 'render-chem' `
    @('render', '--spec', (Join-Path $fix 'sample-chem-render-spec.json'), '--output', $chem) $chem
$results += Invoke-Case 'analyze-basic' `
    @('analyze', '--input', $basic, '--output', (Join-Path $work 'basic-analysis.json'), '--assets', (Join-Path $work 'assets-basic')) `
    (Join-Path $work 'basic-analysis.json')
$results += Invoke-Case 'apply-basic' `
    @('apply', '--source', $basic, '--decision', (Join-Path $fix 'sample-decision.json'), '--output', $applied, '--workdir', (Join-Path $work 'applywork')) `
    $applied

# R2: unified FormatPlan path (plan command -> Compiler -> RenderSpec).
$planGen = Join-Path $work 'plan-generate.docx'
$planOv  = Join-Path $work 'plan-overlay.docx'
$results += Invoke-Case 'plan-generate' `
    @('plan', '--plan', (Join-Path $fix 'sample-format-plan.json'), '--output', $planGen) $planGen
$results += Invoke-Case 'plan-overlay' `
    @('plan', '--plan', (Join-Path $fix 'sample-format-plan-overlay.json'), '--source', $basic, '--output', $planOv, '--workdir', (Join-Path $work 'planwork')) `
    $planOv

$fail = ($results | Where-Object { -not $_ }).Count
Write-Host ""
if ($Mode -eq 'record') {
    Write-Host "RECORDED $($results.Count) snapshots." -ForegroundColor Green
    exit 0
}
if ($fail -gt 0) {
    Write-Host "VERIFY FAILED: $fail / $($results.Count) cases drifted." -ForegroundColor Red
    exit 1
}
Write-Host "VERIFY OK: all $($results.Count) cases match the golden baseline." -ForegroundColor Green
exit 0
