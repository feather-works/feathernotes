//! A hora local (com o fuso do sistema), para o F5. A biblioteca padrão só dá o UTC; o fuso vem
//! do próprio sistema (`localtime_r`, `GetLocalTime`).
use std::time::{SystemTime, UNIX_EPOCH};

/// Uma data e hora no relógio de quem usa.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Local {
    pub ano: i64,
    pub mes: u32,
    pub dia: u32,
    pub hora: u32,
    pub minuto: u32,
}

impl Local {
    /// "02/11/2026 14:30".
    pub fn texto(self) -> String {
        format!("{:02}/{:02}/{} {:02}:{:02}", self.dia, self.mes, self.ano, self.hora, self.minuto)
    }
}

/// Segundos desde 1970 (UTC).
pub fn agora() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// O instante `t` (segundos UTC) no relógio local.
pub fn local(t: i64) -> Local {
    sistema::local(t)
}

#[cfg(unix)]
mod sistema {
    /// O `struct tm` da glibc e da libc do macOS (com `tm_gmtoff` e `tm_zone` no fim).
    #[repr(C)]
    struct Tm {
        sec: i32,
        min: i32,
        hour: i32,
        mday: i32,
        mon: i32,
        year: i32,
        wday: i32,
        yday: i32,
        isdst: i32,
        gmtoff: std::ffi::c_long,
        zone: *const std::ffi::c_char,
    }

    extern "C" {
        fn localtime_r(t: *const i64, tm: *mut Tm) -> *mut Tm;
    }

    pub fn local(t: i64) -> super::Local {
        // SAFETY: ponteiros para valores locais válidos; `localtime_r` só escreve no `tm`.
        let mut tm: Tm = unsafe { std::mem::zeroed() };
        unsafe { localtime_r(&t, &mut tm) };
        super::Local { ano: i64::from(tm.year) + 1900, mes: tm.mon as u32 + 1, dia: tm.mday as u32, hora: tm.hour as u32, minuto: tm.min as u32 }
    }
}

#[cfg(windows)]
mod sistema {
    use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows_sys::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};

    pub fn local(t: i64) -> super::Local {
        // FILETIME: 100 ns desde 1601.
        let v = ((t + 11_644_473_600) * 10_000_000) as u64;
        let ft = FILETIME { dwLowDateTime: v as u32, dwHighDateTime: (v >> 32) as u32 };
        // SAFETY: estruturas locais válidas; o fuso nulo é o do sistema.
        let (mut utc, mut loc): (SYSTEMTIME, SYSTEMTIME) = unsafe { (std::mem::zeroed(), std::mem::zeroed()) };
        unsafe {
            FileTimeToSystemTime(&ft, &mut utc);
            SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut loc);
        }
        super::Local { ano: i64::from(loc.wYear), mes: u32::from(loc.wMonth), dia: u32::from(loc.wDay), hora: u32::from(loc.wHour), minuto: u32::from(loc.wMinute) }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn hora_local() {
        assert_eq!(Local { ano: 2026, mes: 11, dia: 2, hora: 9, minuto: 5 }.texto(), "02/11/2026 09:05");
        // 1970-01-02 12:00 UTC: em qualquer fuso (até ±14 h) ainda é 1 ou 2 de janeiro de 1970.
        let l = local(36 * 3600);
        assert!(l.ano == 1970 && l.mes == 1 && (1..=2).contains(&l.dia) && l.hora < 24 && l.minuto < 60, "{l:?}");
    }
}
