param([string]$Collector = "$PSScriptRoot/../src/windows_security_details.ps1")
$ErrorActionPreference = 'Stop'
$global:pcdiagMockSac = 'Eval'
function Get-MpComputerStatus {
    [pscustomobject]@{ SmartAppControlState = $global:pcdiagMockSac; AntivirusEnabled = $false; ComputerID = 'SECRET'; QuickScanStartTime = [datetime]'1601-01-01'; AntivirusSignatureVersion = '1.2.3' }
}
function Get-MpPreference {
    [pscustomobject]@{ EnableControlledFolderAccess = 2; PUAProtection = 1; EnableNetworkProtection = 0; ExclusionPath = 'SECRET' }
}
function Get-MpThreat { @() }
function Get-NetFirewallProfile {
    [pscustomobject]@{ Name = 'Domain'; Enabled = 'True' }
    [pscustomobject]@{ Name = 'Private'; Enabled = 'False' }
    [pscustomobject]@{ Name = 'Public'; Enabled = 'NotConfigured' }
}
function Get-ProcessMitigation {
    [pscustomobject]@{ DEP = [pscustomobject]@{ Enable = 'NOTSET' } }
}
function Get-Tpm { throw [System.UnauthorizedAccessException]::new('SECRET') }
function Confirm-SecureBootUEFI { $false }
foreach ($case in @(@('On','enabled'), @('Off','disabled'), @('Eval','evaluation'), @('Evaluation','evaluation'), @('FutureValue',$null))) {
    $global:pcdiagMockSac = $case[0]
    $json = & $Collector
    $data = $json | ConvertFrom-Json
    if ($data.smart_app_control.value -ne $case[1]) { throw "SAC normalization failed: $json" }
    if ($data.antivirus_enabled.value -ne 'false') { throw 'False was lost' }
    if ($data.active_threat_count.value -ne '0') { throw 'Zero was lost' }
    if ($data.tpm_ready.status -ne 'permission_denied') { throw 'Permission reason lost' }
    if ($data.quick_scan_start.status -ne 'source_null') { throw 'Sentinel date not rejected' }
    if ($data.controlled_folder_access.value -ne 'audit') { throw 'CFA audit lost' }
    if ($data.firewall_private.value -ne 'disabled') { throw 'Firewall false lost' }
    if ($data.exploit_dep.value -ne 'NOTSET') { throw 'Mitigation default lost' }
    if ($data.secure_boot.value -ne 'false') { throw 'Secure Boot false lost' }
    if ($json -match 'SECRET') { throw 'Sensitive source leaked' }
    if (@($data.PSObject.Properties).Count -ne 33) { throw 'Missing detail keys' }
}
'Security detail PowerShell tests passed'
