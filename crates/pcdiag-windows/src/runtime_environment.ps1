$ErrorActionPreference='Stop'
$limit=5000
$errors=[System.Collections.Generic.List[object]]::new()
function Text($v) { if ($null -eq $v -or [string]::IsNullOrWhiteSpace([string]$v)) { $null } else { [string]$v } }
function Redact-Command($v) {
  $s=Text $v; if ($null -eq $s) { return $null }
  $s=[regex]::Replace($s,'(?i)(--?(?:password|passwd|pwd|token|api[_-]?key|secret)(?:=|\s+))([^\s"'']+|"[^"]*"|''[^'']*'')','$1<redacted>')
  [regex]::Replace($s,'(?i)(\b(?:password|passwd|pwd|token|api[_-]?key|secret)\s*=\s*)([^;\s]+)','$1<redacted>')
}
function Exe-Path($command) {
  $s=Text $command; if ($null -eq $s) { return $null }
  if ($s -match '^\s*"([^"]+)"') { return [Environment]::ExpandEnvironmentVariables($Matches[1]) }
  if ($s -match '^\s*([^\s]+\.exe)') { return [Environment]::ExpandEnvironmentVariables($Matches[1]) }
  $null
}
$binaryCache=@{}
function Binary($path) {
  $p=Text $path; $exists=if ($null -eq $p) {$null} else {[IO.File]::Exists($p)}
  $cacheKey=if ($null -eq $p) {$null} else {$p.ToLowerInvariant()}
  if ($null -ne $cacheKey -and $binaryCache.ContainsKey($cacheKey)) { return $binaryCache[$cacheKey] }
  $publisher=$null; $signature=$null; $hash=$null
  if ($exists) { try { $sig=Get-AuthenticodeSignature -LiteralPath $p -ErrorAction Stop; $signature=[string]$sig.Status; if ($sig.SignerCertificate) {$publisher=$sig.SignerCertificate.Subject} } catch {} ; try {$hash=(Get-FileHash -LiteralPath $p -Algorithm SHA256 -ErrorAction Stop).Hash} catch {} }
  $result=[ordered]@{executable_path=$p;publisher=$publisher;signature_status=$signature;sha256=$hash;file_exists=$exists}
  if ($null -ne $cacheKey) {$binaryCache[$cacheKey]=$result}; $result
}
function Category($items,$total) { [ordered]@{items=@($items);truncated=($total -gt $limit)} }
function Fail($path,$code,$exception) { $errors.Add([ordered]@{path=$path;code=$code;permission_denied=([string]$exception -match '(?i)access.*denied|拒否')}) }

$services=$null
try { $raw=@(Get-CimInstance Win32_Service); $items=@($raw | Select-Object -First $limit | ForEach-Object { $exe=Exe-Path $_.PathName; $dependencies=@($_.Dependencies | Where-Object {$null -ne $_ -and -not [string]::IsNullOrWhiteSpace([string]$_)}); [ordered]@{name=[string]$_.Name;display_name=Text $_.DisplayName;description=Text $_.Description;state=[string]$_.State;start_mode=if ($_.DelayedAutoStart) {'delayed_auto'} else {[string]$_.StartMode};command_line=Redact-Command $_.PathName;account=Text $_.StartName;process_id=if ([uint32]$_.ProcessId -eq 0) {$null} else {[uint32]$_.ProcessId};dependencies=$dependencies;binary=Binary $exe} }); $services=Category $items $raw.Count } catch { Fail '/runtime_environment/services/items' 'services_query_failed' $_; $services=[ordered]@{items=$null;truncated=$false} }

$startup=$null
try { $raw=@(Get-CimInstance Win32_StartupCommand); $items=@($raw | Select-Object -First $limit | ForEach-Object { $exe=Exe-Path $_.Command; [ordered]@{name=[string]$_.Name;enabled=$null;source=[string]$_.Location;command_line=Redact-Command $_.Command;target_user=Text $_.User;binary=Binary $exe} }); $startup=Category $items $raw.Count } catch { Fail '/runtime_environment/startup_applications/items' 'startup_query_failed' $_; $startup=[ordered]@{items=$null;truncated=$false} }

$installed=$null
try {
  $items=[System.Collections.Generic.List[object]]::new()
  foreach ($view in @('Registry64','Registry32')) { try { $base=[Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::LocalMachine,[Microsoft.Win32.RegistryView]::$view); $key=$base.OpenSubKey('SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall'); if ($key) { foreach ($name in $key.GetSubKeyNames()) { $app=$key.OpenSubKey($name); $display=Text ($app.GetValue('DisplayName')); if ($display) { $items.Add([ordered]@{name=$display;version=Text ($app.GetValue('DisplayVersion'));publisher=Text ($app.GetValue('Publisher'));installed_on=Text ($app.GetValue('InstallDate'));install_location=Text ($app.GetValue('InstallLocation'));architecture=if ($view -eq 'Registry64') {'x64'} else {'x86'};kind='desktop';uninstall_command=Redact-Command ($app.GetValue('UninstallString'))}) } } } } catch {} }
  try { foreach ($app in @(Get-AppxPackage -AllUsers -ErrorAction Stop)) { $items.Add([ordered]@{name=[string]$app.Name;version=[string]$app.Version;publisher=Text $app.Publisher;installed_on=$null;install_location=Text $app.InstallLocation;architecture=[string]$app.Architecture;kind='store';uninstall_command=$null}) } } catch { Fail '/runtime_environment/installed_applications/items' 'appx_query_failed' $_ }
  $total=$items.Count; $installed=Category @($items | Select-Object -First $limit) $total
} catch { Fail '/runtime_environment/installed_applications/items' 'installed_apps_query_failed' $_; $installed=[ordered]@{items=$null;truncated=$false} }

$processes=$null
try { $raw=@(Get-CimInstance Win32_Process); $items=@($raw | Select-Object -First $limit | ForEach-Object { $p=$_; $user=$null; $access='complete'; try {$owner=Invoke-CimMethod -InputObject $p -MethodName GetOwner -ErrorAction Stop; if ($owner.ReturnValue -eq 0) {$user=if ($owner.Domain) {"$($owner.Domain)\$($owner.User)"} else {$owner.User}}} catch {$access='partial'}; $exe=Text $p.ExecutablePath; [ordered]@{name=[string]$p.Name;process_id=[uint32]$p.ProcessId;parent_process_id=[uint32]$p.ParentProcessId;command_line=Redact-Command $p.CommandLine;started_at=if ($p.CreationDate) {$p.CreationDate.ToUniversalTime().ToString('o')} else {$null};user=Text $user;cpu_time_ms=[uint64](($p.KernelModeTime+$p.UserModeTime)/10000);working_set_bytes=[uint64]$p.WorkingSetSize;io_bytes=[uint64]($p.ReadTransferCount+$p.WriteTransferCount);access=$access;binary=Binary $exe} }); $processes=Category $items $raw.Count } catch { Fail '/runtime_environment/running_processes/items' 'processes_query_failed' $_; $processes=[ordered]@{items=$null;truncated=$false} }

$tasks=$null
try { $raw=@(Get-ScheduledTask); $items=@($raw | Select-Object -First $limit | ForEach-Object { $task=$_; $info=$null; try {$info=$task | Get-ScheduledTaskInfo -ErrorAction Stop} catch {}; $actions=@($task.Actions | ForEach-Object {$exe=Text $_.Execute; [ordered]@{executable=$exe;arguments=Redact-Command $_.Arguments;working_directory=Text $_.WorkingDirectory;binary=Binary $exe}}); $triggers=@($task.Triggers | ForEach-Object {$_.CimClass.CimClassName} | Where-Object {$null -ne $_ -and -not [string]::IsNullOrWhiteSpace([string]$_)}); $interval=($task.Triggers | ForEach-Object {$_.Repetition.Interval} | Where-Object {$_} | Select-Object -First 1); [ordered]@{name=[string]$task.TaskName;folder=[string]$task.TaskPath;description=Text $task.Description;enabled=([string]$task.State -ne 'Disabled');state=[string]$task.State;triggers=$triggers;actions=$actions;account=Text $task.Principal.UserId;run_level=Text $task.Principal.RunLevel;last_run_at=if ($info -and $info.LastRunTime.Year -gt 1900) {$info.LastRunTime.ToUniversalTime().ToString('o')} else {$null};next_run_at=if ($info -and $info.NextRunTime.Year -gt 1900) {$info.NextRunTime.ToUniversalTime().ToString('o')} else {$null};last_result=if ($info) {[int64]$info.LastTaskResult} else {$null};repetition_interval=Text $interval;hidden=[bool]$task.Settings.Hidden} }); $tasks=Category $items $raw.Count } catch { Fail '/runtime_environment/scheduled_tasks/items' 'scheduled_tasks_query_failed' $_; $tasks=[ordered]@{items=$null;truncated=$false} }

$response=[ordered]@{collection=[ordered]@{services=$services;startup_applications=$startup;installed_applications=$installed;running_processes=$processes;scheduled_tasks=$tasks};errors=@($errors)}
$json=ConvertTo-Json -InputObject $response -Depth 9 -Compress
$bytes=[Text.UTF8Encoding]::new($false).GetBytes($json); $stdout=[Console]::OpenStandardOutput(); $stdout.Write($bytes,0,$bytes.Length)
