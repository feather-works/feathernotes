//! RAM e CPU do próprio app, para o rodapé (o mesmo do launcher).

/// Memória privada do processo em KiB, a mesma conta do Gerenciador de Tarefas: sem as bibliotecas
/// compartilhadas (drivers de vídeo), que o sistema e os outros programas já têm carregadas.
#[cfg(not(windows))]
pub fn rss_kb() -> Option<u64> {
    std::fs::read_to_string("/proc/self/status").ok()?.lines().find(|l| l.starts_with("RssAnon:"))?.split_whitespace().nth(1)?.parse().ok()
}

#[cfg(windows)]
pub fn rss_kb() -> Option<u64> {
    use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX2};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    // SAFETY: estrutura zerada, tamanho correto e o pseudo-handle do próprio processo.
    let mut c: PROCESS_MEMORY_COUNTERS_EX2 = unsafe { std::mem::zeroed() };
    let tamanho = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32;
    c.cb = tamanho;
    // O campo privado só existe do Windows 10 21H2 em diante; antes fica zero e vale o total.
    (unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c as *mut _ as *mut _, tamanho) } != 0)
        .then(|| if c.PrivateWorkingSetSize > 0 { c.PrivateWorkingSetSize } else { c.WorkingSetSize } as u64 / 1024)
}

/// (CPU do processo, CPU da máquina) em ticks do `/proc`; a razão entre duas amostras é o uso.
#[cfg(not(windows))]
pub fn tempos_de_cpu() -> Option<(u64, u64)> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let campos: Vec<&str> = stat.rsplit_once(')')?.1.split_whitespace().collect();
    let processo = campos.get(11)?.parse::<u64>().ok()? + campos.get(12)?.parse::<u64>().ok()?;
    let total = std::fs::read_to_string("/proc/stat").ok()?.lines().next()?.split_whitespace().skip(1).filter_map(|v| v.parse::<u64>().ok()).sum();
    Some((processo, total))
}

/// Windows: (núcleo + usuário do processo, núcleo + usuário do sistema), em 100 ns.
#[cfg(windows)]
pub fn tempos_de_cpu() -> Option<(u64, u64)> {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes, GetSystemTimes};
    let valor = |f: &FILETIME| (u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime);
    let zero = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let (mut criacao, mut saida, mut nucleo, mut usuario) = (zero, zero, zero, zero);
    // SAFETY: ponteiros para estruturas locais válidas.
    if unsafe { GetProcessTimes(GetCurrentProcess(), &mut criacao, &mut saida, &mut nucleo, &mut usuario) } == 0 {
        return None;
    }
    let (mut ocioso, mut nucleo_s, mut usuario_s) = (zero, zero, zero);
    // SAFETY: idem.
    if unsafe { GetSystemTimes(&mut ocioso, &mut nucleo_s, &mut usuario_s) } == 0 {
        return None;
    }
    Some((valor(&nucleo) + valor(&usuario), valor(&nucleo_s) + valor(&usuario_s)))
}
