# Starts (or stops) the GhidraMCP headless server on 127.0.0.1 with noita_files/ghidra/noita.gpr and
# noita.exe loaded, so no Ghidra window is needed. Usage: scripts/ghidra-server.ps1 [-Stop]
param(
    [string]$GhidraHome = 'C:\Users\alfie\Documents\ghidra\ghidra_12.1.4_PUBLIC_20260921\ghidra_12.1.4_PUBLIC',
    [string]$PluginJar = 'C:\Users\alfie\Documents\ghidra\ghidraMCP\ghidra-mcp\target\GhidraMCP-7.0.0.jar',
    [string]$JavaHome = $(if ($env:JAVA_HOME) { $env:JAVA_HOME } else { 'C:\Program Files\Eclipse Adoptium\jdk-21.0.7.6-hotspot' }),
    [int]$Port = 8089,
    [int]$TimeoutSeconds = 300,
    [switch]$Stop
)
$ErrorActionPreference = 'Stop'

$repo = Split-Path -Parent $PSScriptRoot
$projectDir = Join-Path $repo 'noita_files\ghidra'
$log = Join-Path $projectDir 'server.log'
$url = "http://127.0.0.1:$Port"

function Get-Status {
    try { (Invoke-WebRequest -UseBasicParsing "$url/check_connection" -TimeoutSec 3).Content } catch { $null }
}

function Test-ProgramLoaded {
    try {
        $open = (Invoke-WebRequest -UseBasicParsing "$url/list_open_programs" -TimeoutSec 5).Content
        $open -match 'noita\.exe'
    } catch { $false }
}

function Get-ServerProcess {
    Get-CimInstance Win32_Process -Filter "Name = 'java.exe' OR Name = 'javaw.exe'" |
        Where-Object { $_.CommandLine -match 'GhidraMCPHeadlessServer|ghidra-mcp-headless\.args' }
}

if ($Stop) {
    $procs = @(Get-ServerProcess)
    foreach ($p in $procs) { Stop-Process -Id $p.ProcessId -Force }
    Write-Output "Stopped $($procs.Count) headless server process(es)."
    exit 0
}

$status = Get-Status
if ($status -and (Test-ProgramLoaded)) {
    Write-Output "Already running: $status"
    exit 0
}
if ($status) {
    Write-Error "Port $Port is in use without noita.exe loaded ($status). Run with -Stop or close the Ghidra window first."
}
if (-not (Test-Path (Join-Path $projectDir 'noita.gpr'))) {
    Write-Error "Project not found: $projectDir\noita.gpr (create it with analyzeHeadless first)."
}

# Java argfiles treat backslashes as escapes, so every path uses forward slashes.
$fwd = { param($p) $p -replace '\\', '/' }
$libs = foreach ($group in 'Framework', 'Features', 'Processors') {
    Get-ChildItem (Join-Path $GhidraHome "Ghidra\$group") -Directory |
        Where-Object { Test-Path (Join-Path $_.FullName 'lib') } |
        ForEach-Object { (& $fwd (Join-Path $_.FullName 'lib')) + '/*' }
}
$classpath = (@(& $fwd $PluginJar) + $libs) -join ';'
$argFile = Join-Path $env:TEMP 'ghidra-mcp-headless.args'
@(
    '-Xmx4g'
    '-XX:+UseG1GC'
    "-Dghidra.home=`"$(& $fwd $GhidraHome)`""
    '-Dapplication.name=GhidraMCP'
    '-cp'
    "`"$classpath`""
    'com.xebyte.headless.GhidraMCPHeadlessServer'
    '--port'
    "$Port"
    '--bind'
    '127.0.0.1'
    '--project'
    "`"$(& $fwd (Join-Path $projectDir 'noita.gpr'))`""
    '--program'
    '/noita.exe'
) | Set-Content -Encoding ascii $argFile

$java = Join-Path $JavaHome 'bin\java.exe'
Start-Process -FilePath $java -ArgumentList "@`"$argFile`"" -WindowStyle Hidden `
    -RedirectStandardOutput $log -RedirectStandardError "$log.err" | Out-Null

$deadline = (Get-Date).AddSeconds($TimeoutSeconds)
while ((Get-Date) -lt $deadline) {
    if (Test-ProgramLoaded) {
        Write-Output "Ready: noita.exe loaded on $url"
        exit 0
    }
    if (-not (Get-ServerProcess)) {
        Write-Error "Server exited during startup; see $log and $log.err"
    }
    Start-Sleep -Seconds 2
}
Write-Error "Timed out after $TimeoutSeconds s waiting for noita.exe to load; see $log"
