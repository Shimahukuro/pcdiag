# Read-only queries. Never serialize source objects, exception text, account IDs or threat paths.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
$result = @{}
function Save-Value($key, $value) {
    if ($null -eq $value -or [string]$value -eq '') {
        $result[$key] = @{ value = $null; status = 'source_null' }
    } else {
        if ($value -is [datetime]) {
            if ($value.Year -lt 2000) {
                $result[$key] = @{ value = $null; status = 'source_null' }
                return
            }
            $value = $value.ToUniversalTime().ToString('o')
        } elseif ($value -is [bool]) {
            $value = $value.ToString().ToLowerInvariant()
        }
        $result[$key] = @{ value = [string]$value; status = $null }
    }
}
function Read-Group($keys, [scriptblock]$query) {
    try { & $query | Out-Null } catch {
        $status = 'failed'
        if ($_.Exception -is [System.UnauthorizedAccessException] -or $_.Exception.HResult -eq -2147024891) { $status = 'permission_denied' }
        elseif ($_.Exception -is [System.Management.Automation.CommandNotFoundException]) { $status = 'unsupported' }
        elseif ($_.Exception -is [Microsoft.Management.Infrastructure.CimException]) {
            switch ([int]$_.Exception.NativeErrorCode) {
                2 { $status = 'permission_denied' }
                3 { $status = 'unsupported' }
                5 { $status = 'unsupported' }
                7 { $status = 'unsupported' }
            }
        }
        foreach ($key in $keys) {
            if (-not $result.ContainsKey($key)) { $result[$key] = @{ value = $null; status = $status } }
        }
    }
}
$mpFields = @{
    smart_app_control = 'SmartAppControlState'
    defender_mode = 'AMRunningMode'
    antivirus_enabled = 'AntivirusEnabled'
    realtime_protection = 'RealTimeProtectionEnabled'
    behavior_monitor = 'BehaviorMonitorEnabled'
    ioav_protection = 'IoavProtectionEnabled'
    tamper_protection = 'IsTamperProtected'
    signature_version = 'AntivirusSignatureVersion'
    signature_updated = 'AntivirusSignatureLastUpdated'
    signatures_outdated = 'DefenderSignaturesOutOfDate'
    quick_scan_start = 'QuickScanStartTime'
    quick_scan_end = 'QuickScanEndTime'
    full_scan_start = 'FullScanStartTime'
    full_scan_end = 'FullScanEndTime'
}
Read-Group @($mpFields.Keys) {
    $mp = Get-MpComputerStatus -ErrorAction Stop
    foreach ($key in $mpFields.Keys) {
        $value = $mp.($mpFields[$key])
        if ($key -eq 'smart_app_control' -and $null -ne $value) {
            $value = switch ([string]$value) {
                'On' { 'enabled' }
                'Off' { 'disabled' }
                'Evaluation' { 'evaluation' }
                'Eval' { 'evaluation' }
                default { '__invalid__' }
            }
            if ($value -eq '__invalid__') {
                $result[$key] = @{ value = $null; status = 'invalid_value' }
                continue
            }
        }
        Save-Value $key $value
    }
}
$preferenceFields = @{
    controlled_folder_access = 'EnableControlledFolderAccess'
    pua_protection = 'PUAProtection'
    network_protection = 'EnableNetworkProtection'
}
Read-Group @($preferenceFields.Keys) {
    $preferences = Get-MpPreference -ErrorAction Stop
    foreach ($key in $preferenceFields.Keys) {
        $raw = $preferences.($preferenceFields[$key])
        $value = $null
        if ($null -ne $raw) {
            $value = switch ([int]$raw) {
                0 { 'disabled' }
                1 { 'enabled' }
                2 { 'audit' }
                3 { if ($key -eq 'controlled_folder_access') { 'block_disk_modification_only' } }
                4 { if ($key -eq 'controlled_folder_access') { 'audit_disk_modification_only' } }
            }
        }
        Save-Value $key $value
        if ($null -ne $raw -and $null -eq $value) {
            $result[$key] = @{ value = $null; status = 'invalid_value' }
        }
    }
}
Read-Group @('active_threat_count') {
    $threats = @(Get-MpThreat -ErrorAction Stop)
    # Count only; never save resource paths, IDs or user names.
    if (@($threats | Where-Object { $null -eq $_.IsActive }).Count -gt 0) {
        Save-Value 'active_threat_count' $null
    } else {
        Save-Value 'active_threat_count' @($threats | Where-Object { $_.IsActive -eq $true }).Count
    }
}
Read-Group @('firewall_domain', 'firewall_private', 'firewall_public') {
    $profiles = @(Get-NetFirewallProfile -PolicyStore ActiveStore -ErrorAction Stop)
    foreach ($name in @('Domain', 'Private', 'Public')) {
        $profile = @($profiles | Where-Object { [string]$_.Name -eq $name })
        $value = $null
        if ($profile.Count -eq 1 -and $null -ne $profile[0].Enabled) {
            $value = switch ([string]$profile[0].Enabled) {
                'True' { 'enabled' }
                'False' { 'disabled' }
                'NotConfigured' { 'not_configured' }
            }
        }
        Save-Value ('firewall_' + $name.ToLowerInvariant()) $value
    }
}
Read-Group @('active_firewall_profiles') {
    $policy = New-Object -ComObject HNetCfg.FwPolicy2
    try {
        $mask = [int]$policy.CurrentProfileTypes
        $names = @()
        if ($mask -band 1) { $names += 'domain' }
        if ($mask -band 2) { $names += 'private' }
        if ($mask -band 4) { $names += 'public' }
        if ($mask -eq 0) { $names += 'none' }
        Save-Value 'active_firewall_profiles' ($names -join ', ')
    } finally { [void][System.Runtime.InteropServices.Marshal]::FinalReleaseComObject($policy) }
}
$mitigationFields = @{
    exploit_dep = @('DEP', 'Enable')
    exploit_cfg = @('CFG', 'Enable')
    exploit_aslr_bottom_up = @('ASLR', 'BottomUp')
    exploit_aslr_force = @('ASLR', 'ForceRelocateImages')
    exploit_sehop = @('SEHOP', 'Enable')
    exploit_heap = @('Heap', 'TerminateOnError')
}
Read-Group @($mitigationFields.Keys) {
    $mitigation = Get-ProcessMitigation -System -ErrorAction Stop
    foreach ($key in $mitigationFields.Keys) {
        $parts = $mitigationFields[$key]
        Save-Value $key $mitigation.($parts[0]).($parts[1])
    }
}
$tpmFields = @{ tpm_present = 'TpmPresent'; tpm_ready = 'TpmReady'; tpm_enabled = 'TpmEnabled'; tpm_locked_out = 'LockedOut' }
Read-Group @($tpmFields.Keys) {
    $tpm = Get-Tpm -ErrorAction Stop
    foreach ($key in $tpmFields.Keys) { Save-Value $key $tpm.($tpmFields[$key]) }
}
Read-Group @('secure_boot') {
    Save-Value 'secure_boot' (Confirm-SecureBootUEFI -ErrorAction Stop)
}
ConvertTo-Json -InputObject $result -Depth 4 -Compress
