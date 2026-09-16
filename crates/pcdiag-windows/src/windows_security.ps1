$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
$result = @{ configured = $null; running = $null; error = $null }
try {
    $instances = @(Get-CimInstance -Namespace 'root\Microsoft\Windows\DeviceGuard' -ClassName 'Win32_DeviceGuard' -Property SecurityServicesConfigured, SecurityServicesRunning -OperationTimeoutSec 30 -ErrorAction Stop)
    if ($instances.Count -eq 1) {
        $result.configured = $instances[0].SecurityServicesConfigured
        $result.running = $instances[0].SecurityServicesRunning
    }
} catch {
    $status = 'failed'
    if ($_.Exception -is [Microsoft.Management.Infrastructure.CimException]) {
        switch ([int]$_.Exception.NativeErrorCode) {
            2 { $status = 'permission_denied' }
            3 { $status = 'unsupported' }
            5 { $status = 'unsupported' }
            7 { $status = 'unsupported' }
        }
    } elseif ($_.Exception -is [System.UnauthorizedAccessException]) {
        $status = 'permission_denied'
    }
    $result.error = @{ status = $status; native_code = [long]$_.Exception.HResult }
}
ConvertTo-Json -InputObject $result -Depth 4 -Compress
