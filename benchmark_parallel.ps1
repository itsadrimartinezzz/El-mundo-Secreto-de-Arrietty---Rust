# Ejecutar desde la raiz. No correr otras copias del diorama durante la medicion.
$ErrorActionPreference = 'Stop'
cargo build --release --example benchmark_parallel
if ($LASTEXITCODE -ne 0) { throw 'Fallo la compilacion' }
$previousThreads = $env:ARRIETTY_THREADS
try {
    New-Item -ItemType Directory -Force renders | Out-Null
    'threads,view,trace_ms,total_ms,min_ms,max_ms,checksum' | Set-Content renders/parallel_benchmark.csv
    foreach ($count in (@(1,2,4,8,16,[Environment]::ProcessorCount) | Sort-Object -Unique)) {
        $env:ARRIETTY_THREADS = "$count"
        & .\target\release\examples\benchmark_parallel.exe | Tee-Object -FilePath renders/parallel_benchmark.csv -Append
        if ($LASTEXITCODE -ne 0) { throw 'Fallo el benchmark' }
    }
} finally {
    if ($null -eq $previousThreads) { Remove-Item Env:ARRIETTY_THREADS -ErrorAction SilentlyContinue }
    else { $env:ARRIETTY_THREADS = $previousThreads }
}
