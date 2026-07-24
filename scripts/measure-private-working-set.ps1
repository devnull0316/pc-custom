[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('A-Tauri', 'B-Electron-main', 'B-Native-watcher')]
    [string]$Variant,

    [Parameter(Mandatory = $true)]
    [ValidateSet('ui-open', 'ui-closed-native-core')]
    [string]$Scenario,

    [Parameter(Mandatory = $true)]
    [ValidateRange(1, 2147483647)]
    [int]$RootProcessId,

    [ValidateRange(0, 3600)]
    [int]$WarmupSeconds = 60,

    [ValidateRange(10, 3600)]
    [int]$SampleSeconds = 120,

    [ValidateRange(100, 60000)]
    [int]$IntervalMilliseconds = 1000,

    [string]$OutputDirectory = (Join-Path (Split-Path -Parent $PSScriptRoot) 'artifacts\memory')
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-ProcessTreeIds {
    param(
        [Parameter(Mandatory = $true)]
        [int]$RootId
    )

    $processRows = @(Get-CimInstance -ClassName Win32_Process -ErrorAction Stop)
    $childrenByParent = @{}
    foreach ($row in $processRows) {
        $parentId = [int]$row.ParentProcessId
        if (-not $childrenByParent.ContainsKey($parentId)) {
            $childrenByParent[$parentId] = [System.Collections.Generic.List[int]]::new()
        }
        $childrenByParent[$parentId].Add([int]$row.ProcessId)
    }

    $foundRoot = $processRows | Where-Object { [int]$_.ProcessId -eq $RootId } | Select-Object -First 1
    if ($null -eq $foundRoot) {
        throw "Root process is not running."
    }

    $visited = [System.Collections.Generic.HashSet[int]]::new()
    $pending = [System.Collections.Generic.Queue[int]]::new()
    $pending.Enqueue($RootId)

    while ($pending.Count -gt 0) {
        $currentId = $pending.Dequeue()
        if (-not $visited.Add($currentId)) {
            continue
        }
        if ($childrenByParent.ContainsKey($currentId)) {
            foreach ($childId in $childrenByParent[$currentId]) {
                $pending.Enqueue($childId)
            }
        }
    }

    return @($visited | Sort-Object)
}

function Get-Percentile {
    param(
        [Parameter(Mandatory = $true)]
        [double[]]$Values,

        [Parameter(Mandatory = $true)]
        [ValidateRange(0, 100)]
        [double]$Percentile
    )

    if ($Values.Count -eq 0) {
        throw "Cannot calculate a percentile from an empty sample."
    }
    $sorted = @($Values | Sort-Object)
    $index = [Math]::Ceiling(($Percentile / 100.0) * $sorted.Count) - 1
    $index = [Math]::Max(0, [Math]::Min($sorted.Count - 1, $index))
    return [double]$sorted[$index]
}

function Get-Median {
    param(
        [Parameter(Mandatory = $true)]
        [double[]]$Values
    )

    if ($Values.Count -eq 0) {
        throw "Cannot calculate a median from an empty sample."
    }
    $sorted = @($Values | Sort-Object)
    $middle = [Math]::Floor($sorted.Count / 2)
    if (($sorted.Count % 2) -eq 1) {
        return [double]$sorted[$middle]
    }
    return ([double]$sorted[$middle - 1] + [double]$sorted[$middle]) / 2.0
}

function New-MetricSummary {
    param(
        [Parameter(Mandatory = $true)]
        [double[]]$Values
    )

    $measurement = $Values | Measure-Object -Minimum -Maximum -Average
    return [ordered]@{
        minimum = [double]$measurement.Minimum
        median  = Get-Median -Values $Values
        p95     = Get-Percentile -Values $Values -Percentile 95
        maximum = [double]$measurement.Maximum
        average = [double]$measurement.Average
    }
}

try {
    $rootProcess = Get-Process -Id $RootProcessId -ErrorAction Stop
    $rootStartTimeUtc = $rootProcess.StartTime.ToUniversalTime()

    $fullOutputDirectory = [System.IO.Path]::GetFullPath($OutputDirectory)
    $null = New-Item -ItemType Directory -Path $fullOutputDirectory -Force -ErrorAction Stop

    if ($WarmupSeconds -gt 0) {
        Start-Sleep -Seconds $WarmupSeconds
    }

    $sampleCount = [Math]::Ceiling(($SampleSeconds * 1000.0) / $IntervalMilliseconds)
    $samples = [System.Collections.Generic.List[object]]::new()

    for ($sampleIndex = 0; $sampleIndex -lt $sampleCount; $sampleIndex++) {
        $currentRootProcess = Get-Process -Id $RootProcessId -ErrorAction Stop
        if ($currentRootProcess.StartTime.ToUniversalTime() -ne $rootStartTimeUtc) {
            throw "The root PID was reused by a different process."
        }
        $treeIds = @(Get-ProcessTreeIds -RootId $RootProcessId)
        $treeIdSet = [System.Collections.Generic.HashSet[int]]::new()
        foreach ($treeId in $treeIds) {
            $null = $treeIdSet.Add([int]$treeId)
        }

        $perfRows = @(
            Get-CimInstance -ClassName Win32_PerfFormattedData_PerfProc_Process -ErrorAction Stop |
                Where-Object { $treeIdSet.Contains([int]$_.IDProcess) }
        )
        if (-not ($perfRows | Where-Object { [int]$_.IDProcess -eq $RootProcessId })) {
            throw "The root process disappeared from the performance snapshot."
        }

        $privateWorkingSetBytes = 0.0
        $workingSetBytes = 0.0
        $handleCount = 0.0
        $threadCount = 0.0
        $processorPercent = 0.0
        foreach ($row in $perfRows) {
            $privateWorkingSetBytes += [double]$row.WorkingSetPrivate
            $workingSetBytes += [double]$row.WorkingSet
            $handleCount += [double]$row.HandleCount
            $threadCount += [double]$row.ThreadCount
            $processorPercent += [double]$row.PercentProcessorTime
        }

        $samples.Add([pscustomobject][ordered]@{
            timestamp_utc                    = [DateTime]::UtcNow.ToString('o')
            sample_index                     = $sampleIndex
            root_process_id                  = $RootProcessId
            process_count                    = $perfRows.Count
            descendant_count                 = [Math]::Max(0, $perfRows.Count - 1)
            private_working_set_bytes        = [int64]$privateWorkingSetBytes
            working_set_bytes                = [int64]$workingSetBytes
            handle_count                     = [int64]$handleCount
            thread_count                     = [int64]$threadCount
            processor_percent_all_logical_cpu = [double]$processorPercent
        })

        if ($sampleIndex -lt ($sampleCount - 1)) {
            Start-Sleep -Milliseconds $IntervalMilliseconds
        }
    }

    $timestamp = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ')
    $baseName = "${timestamp}_${Variant}_${Scenario}"
    $rawPath = Join-Path $fullOutputDirectory "${baseName}_raw.csv"
    $summaryPath = Join-Path $fullOutputDirectory "${baseName}_summary.json"

    $samples | Export-Csv -LiteralPath $rawPath -NoTypeInformation -Encoding UTF8

    $descendantMaximum = [int](($samples.descendant_count | Measure-Object -Maximum).Maximum)
    $singleProcessGatePassed = ($Scenario -ne 'ui-closed-native-core') -or ($descendantMaximum -eq 0)
    $summary = [ordered]@{
        schema_version = 1
        captured_at_utc = [DateTime]::UtcNow.ToString('o')
        variant = $Variant
        scenario = $Scenario
        root_process_id = $RootProcessId
        warmup_seconds = $WarmupSeconds
        sample_seconds = $SampleSeconds
        interval_milliseconds = $IntervalMilliseconds
        sample_count = $samples.Count
        process_tree_scope = 'root process plus current descendants; command lines and paths are not collected'
        ui_closed_native_core_gate = [ordered]@{
            applicable = ($Scenario -eq 'ui-closed-native-core')
            passed = $singleProcessGatePassed
            maximum_descendant_count = $descendantMaximum
        }
        metrics = [ordered]@{
            private_working_set_bytes = New-MetricSummary -Values ([double[]]$samples.private_working_set_bytes)
            working_set_bytes = New-MetricSummary -Values ([double[]]$samples.working_set_bytes)
            process_count = New-MetricSummary -Values ([double[]]$samples.process_count)
            handle_count = New-MetricSummary -Values ([double[]]$samples.handle_count)
            thread_count = New-MetricSummary -Values ([double[]]$samples.thread_count)
            processor_percent_all_logical_cpu = New-MetricSummary -Values ([double[]]$samples.processor_percent_all_logical_cpu)
        }
    }

    $summary | ConvertTo-Json -Depth 8 | Out-File -LiteralPath $summaryPath -Encoding UTF8

    Write-Output "Raw samples: $rawPath"
    Write-Output "Summary: $summaryPath"

    if (-not $singleProcessGatePassed) {
        [Console]::Error.WriteLine("ui-closed-native-core retained one or more descendant processes; this run is invalid for the native-core-only comparison.")
        exit 2
    }

    exit 0
}
catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
}
