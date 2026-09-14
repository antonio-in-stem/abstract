[CmdletBinding()]
param(
    [string]$AbstractExe,
    [string]$CueExe
)

$ErrorActionPreference = 'Stop'

if (-not $AbstractExe) {
    $AbstractExe = Join-Path $PSScriptRoot '../../../target/release/abstract.exe'
}
if (-not $CueExe) {
    $cueCommand = Get-Command cue -CommandType Application -ErrorAction Stop
    $CueExe = $cueCommand.Source
}

function Invoke-CheckedTool {
    param(
        [Parameter(Mandatory)] [string]$Executable,
        [Parameter(Mandatory)] [string[]]$Arguments
    )

    $previousErrorPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        $output = (& $Executable @Arguments 2>&1 | ForEach-Object { $_.ToString() } | Out-String).TrimEnd()
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousErrorPreference
    }
    [pscustomobject]@{
        ExitCode = $exitCode
        Output = $output
    }
}

function ConvertTo-NormalForm {
    param([Parameter(ValueFromPipeline)] $Value)

    if ($null -eq $Value) {
        return $null
    }
    if ($Value -is [System.Management.Automation.PSCustomObject]) {
        $ordered = [ordered]@{}
        foreach ($property in ($Value.PSObject.Properties | Sort-Object Name)) {
            $ordered[$property.Name] = ConvertTo-NormalForm $property.Value
        }
        return [pscustomobject]$ordered
    }
    if ($Value -is [System.Collections.IEnumerable] -and $Value -isnot [string]) {
        return @($Value | ForEach-Object { ConvertTo-NormalForm $_ })
    }
    return $Value
}

function ConvertTo-CanonicalJson {
    param([Parameter(Mandatory)] $Value)
    ConvertTo-NormalForm $Value | ConvertTo-Json -Depth 100 -Compress
}

function Assert-Success {
    param([string]$Name, $Result)
    if ($Result.ExitCode -ne 0) {
        throw "$Name failed with exit code $($Result.ExitCode):`n$($Result.Output)"
    }
}

function Assert-Rejection {
    param([string]$Name, $Result, [string]$Pattern)
    if ($Result.ExitCode -eq 0) {
        throw "$Name unexpectedly accepted invalid input."
    }
    if ($Result.Output -notmatch $Pattern) {
        throw "$Name rejected the input for an unexpected reason:`n$($Result.Output)"
    }
}

$AbstractExe = (Resolve-Path -LiteralPath $AbstractExe).Path
$CueExe = (Resolve-Path -LiteralPath $CueExe).Path
$schema = Join-Path $PSScriptRoot 'abstract/schema/Invoice.abt'
$abstractCases = Join-Path $PSScriptRoot 'abstract/cases'
$cueSchema = Join-Path $PSScriptRoot 'cue/schema.cue'
$cueCases = Join-Path $PSScriptRoot 'cue/cases'
$expectedPath = Join-Path $PSScriptRoot 'expected.normalized.json'

$abstractVersion = Invoke-CheckedTool $AbstractExe @('--version')
$cueVersion = Invoke-CheckedTool $CueExe @('version')
Assert-Success 'Abstract version check' $abstractVersion
Assert-Success 'CUE version check' $cueVersion
if ($abstractVersion.Output -notmatch '^abstract 1\.4\.0$') {
    throw "Expected Abstract 1.4.0, got: $($abstractVersion.Output)"
}
if ($cueVersion.Output -notmatch '^cue version v0\.17\.1') {
    throw "Expected CUE v0.17.1, got: $($cueVersion.Output)"
}

$scratchName = 'abstract-cue-comparison-' + [guid]::NewGuid().ToString('N')
$scratchRoot = Join-Path ([System.IO.Path]::GetTempPath()) $scratchName
$templateDir = Join-Path $scratchRoot 'data/templates'
$invoiceDir = Join-Path $scratchRoot 'data/invoices'
New-Item -ItemType Directory -Path $templateDir -Force | Out-Null
New-Item -ItemType Directory -Path $invoiceDir -Force | Out-Null

try {
    Copy-Item -LiteralPath $schema -Destination (Join-Path $templateDir 'Invoice.abt')
    Copy-Item -LiteralPath (Join-Path $abstractCases 'valid.ab') -Destination (Join-Path $invoiceDir 'case.ab')

    $abstractValid = Invoke-CheckedTool $AbstractExe @('compile', $scratchRoot, 'JSON')
    $cueValid = Invoke-CheckedTool $CueExe @('export', $cueSchema, (Join-Path $cueCases 'valid.cue'), '-e', 'invoice')
    Assert-Success 'Abstract valid case' $abstractValid
    Assert-Success 'CUE valid case' $cueValid

    $abstractDocument = $abstractValid.Output | ConvertFrom-Json
    if ($abstractDocument.data.Count -ne 1) {
        throw "Expected one Abstract data record, got $($abstractDocument.data.Count)."
    }
    $abstractInvoice = $abstractDocument.data[0]
    $abstractInvoice.PSObject.Properties.Remove('template')
    $cueInvoice = $cueValid.Output | ConvertFrom-Json
    $expectedInvoice = Get-Content -Raw -LiteralPath $expectedPath | ConvertFrom-Json

    $abstractJson = ConvertTo-CanonicalJson $abstractInvoice
    $cueJson = ConvertTo-CanonicalJson $cueInvoice
    $expectedJson = ConvertTo-CanonicalJson $expectedInvoice
    if ($abstractJson -ne $cueJson -or $abstractJson -ne $expectedJson) {
        throw "Normalized output mismatch.`nAbstract: $abstractJson`nCUE: $cueJson`nExpected: $expectedJson"
    }
    Write-Output 'PASS valid: Abstract and CUE equal expected.normalized.json'

    $negativeCases = @(
        @{ Name = 'invalid-enum'; Abstract = 'error\[E414\]'; Cue = 'invoice\.status:.*empty disjunction' },
        @{ Name = 'invalid-nested'; Abstract = 'error\[E411\]'; Cue = 'invoice\.customer\.address\.city: field is required but not present' },
        @{ Name = 'invalid-bound'; Abstract = 'error\[E413\]'; Cue = 'invoice\.lines\.0\.quantity: invalid value 0 \(out of bound >=1\)' },
        @{ Name = 'invalid-rule'; Abstract = 'error\[E515\]'; Cue = 'invoice\.total_cents: invalid value 600000 \(out of bound <=500000\)' }
    )

    foreach ($case in $negativeCases) {
        Copy-Item -LiteralPath (Join-Path $abstractCases ($case.Name + '.ab')) -Destination (Join-Path $invoiceDir 'case.ab') -Force
        $abstractInvalid = Invoke-CheckedTool $AbstractExe @('lint', $scratchRoot)
        $cueInvalid = Invoke-CheckedTool $CueExe @('vet', '-c', $cueSchema, (Join-Path $cueCases ($case.Name + '.cue')))
        Assert-Rejection "Abstract $($case.Name)" $abstractInvalid $case.Abstract
        Assert-Rejection "CUE $($case.Name)" $cueInvalid $case.Cue
        Write-Output "PASS $($case.Name): both tools reject the intended constraint"
    }

    Write-Output 'PASS all comparison checks (Abstract 1.4.0, CUE v0.17.1)'
}
finally {
    $resolvedTemp = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
    $resolvedScratch = [System.IO.Path]::GetFullPath($scratchRoot)
    if ($resolvedScratch.StartsWith($resolvedTemp, [System.StringComparison]::OrdinalIgnoreCase) -and
        ([System.IO.Path]::GetFileName($resolvedScratch)).StartsWith('abstract-cue-comparison-', [System.StringComparison]::Ordinal)) {
        Remove-Item -LiteralPath $resolvedScratch -Recurse -Force -ErrorAction SilentlyContinue
    }
}
