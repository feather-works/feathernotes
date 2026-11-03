//! A hora local (com o fuso do sistema), para o F5 e os lembretes dos post-its. A biblioteca
//! padrão só dá o UTC; o fuso vem do próprio sistema (`localtime_r`, `GetLocalTime`).
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

    /// Os segundos desde 1970 desta data como se fosse UTC (o fuso se tira à parte).
    fn segundos(self) -> i64 {
        dias(self.ano, self.mes, self.dia) * 86_400 + i64::from(self.hora) * 3600 + i64::from(self.minuto) * 60
    }
}

/// Segundos desde 1970 (UTC).
pub fn agora() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// Dias de 1970-01-01 até a data (algoritmo de Howard Hinnant, calendário gregoriano).
fn dias(ano: i64, mes: u32, dia: u32) -> i64 {
    let a = if mes <= 2 { ano - 1 } else { ano };
    let era = a.div_euclid(400);
    let ano_da_era = a - era * 400;
    let m = i64::from(mes);
    let dia_do_ano = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(dia) - 1;
    let dia_da_era = ano_da_era * 365 + ano_da_era / 4 - ano_da_era / 100 + dia_do_ano;
    era * 146_097 + dia_da_era - 719_468
}

/// O instante `t` (segundos UTC) no relógio local.
pub fn local(t: i64) -> Local {
    sistema::local(t)
}

/// Quantos segundos o relógio local está à frente do UTC agora (-10800 no Brasil).
fn fuso() -> i64 {
    let t = agora();
    // Sem os segundos no `Local`: o resto fica fora da conta.
    sistema::local(t).segundos() - (t - t.rem_euclid(60))
}

/// Uma data e hora do relógio local em segundos UTC.
// shortcut: usa o fuso de agora; numa data depois de uma troca de horário de verão erra por 1 h. O Brasil não tem mais horário de verão.
fn de_local(ano: i64, mes: u32, dia: u32, hora: u32, minuto: u32) -> i64 {
    Local { ano, mes, dia, hora, minuto }.segundos() - fuso()
}

/// "sáb 15/11 às 14:30": como o lembrete foi entendido.
pub fn descrever(t: i64) -> String {
    let l = local(t);
    let semana = ["dom", "seg", "ter", "qua", "qui", "sex", "sáb"][(dias(l.ano, l.mes, l.dia) + 4).rem_euclid(7) as usize];
    format!("{semana} {:02}/{:02} às {:02}:{:02}", l.dia, l.mes, l.hora, l.minuto)
}

/// O que se escreve no lembrete: "15/11 14:30", "15/11/2027 9h", "amanhã 8h", "18:00" (hoje, ou
/// amanhã se já passou), "15/11" (às 9h). `None` se não entendeu ou se já passou.
pub fn interpretar(texto: &str, agora: i64) -> Option<i64> {
    let hoje = local(agora);
    let (mut data, mut horario) = (None, None);
    for parte in texto.split_whitespace().map(str::to_lowercase) {
        match parte.as_str() {
            "hoje" => data = Some((hoje.ano, hoje.mes, hoje.dia)),
            "amanhã" | "amanha" => {
                let l = local(de_local(hoje.ano, hoje.mes, hoje.dia, 12, 0) + 86_400);
                data = Some((l.ano, l.mes, l.dia));
            }
            p if p.contains('/') => {
                let n: Vec<&str> = p.split('/').collect();
                let (dia, mes) = (n.first()?.parse::<u32>().ok()?, n.get(1)?.parse::<u32>().ok()?);
                let ano = match n.get(2) {
                    Some(a) if a.len() == 2 => 2000 + a.parse::<i64>().ok()?,
                    Some(a) => a.parse().ok()?,
                    None => hoje.ano,
                };
                let dias_no_mes = (dias(if mes == 12 { ano + 1 } else { ano }, if mes == 12 { 1 } else { mes + 1 }, 1) - dias(ano, mes, 1)) as u32;
                if !(1..=12).contains(&mes) || dia == 0 || dia > dias_no_mes || n.len() > 3 {
                    return None;
                }
                data = Some((ano, mes, dia));
            }
            p => {
                let (h, m) = p.split_once([':', 'h']).unwrap_or((p, ""));
                let (h, m) = (h.parse::<u32>().ok()?, if m.is_empty() { 0 } else { m.parse::<u32>().ok()? });
                if h > 23 || m > 59 {
                    return None;
                }
                horario = Some((h, m));
            }
        }
    }
    let (h, m) = horario.unwrap_or((9, 0));
    let t = match data {
        Some((ano, mes, dia)) => de_local(ano, mes, dia, h, m),
        None if horario.is_some() => {
            // Só a hora: hoje, ou amanhã se já passou.
            let t = de_local(hoje.ano, hoje.mes, hoje.dia, h, m);
            if t > agora { t } else { t + 86_400 }
        }
        None => return None,
    };
    // 15/11 sem ano já passou este ano: é o do ano que vem.
    let t = if t <= agora && data.is_some() && texto.matches('/').count() < 2 { local_ano_que_vem(t) } else { t };
    (t > agora).then_some(t)
}

/// O mesmo dia e hora no ano seguinte.
fn local_ano_que_vem(t: i64) -> i64 {
    let l = local(t);
    de_local(l.ano + 1, l.mes, l.dia, l.hora, l.minuto)
}

/// Amanhã às `hora` em ponto, em segundos UTC.
pub fn amanha_as(hora: u32) -> i64 {
    let hoje = local(agora());
    dias(hoje.ano, hoje.mes, hoje.dia) * 86_400 + 86_400 + i64::from(hora) * 3600 - fuso()
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
    fn dias_e_fuso() {
        assert_eq!(dias(1970, 1, 1), 0);
        assert_eq!(dias(2000, 3, 1), 11_017);
        assert_eq!(dias(2026, 11, 2), 20_759);
        // O fuso é de minutos inteiros e no máximo 14 h.
        let f = fuso();
        assert!(f % 60 == 0 && f.abs() <= 14 * 3600, "{f}");
        let amanha = amanha_as(9);
        assert!(amanha > agora() && amanha - agora() <= 33 * 3600);
        assert_eq!((local(amanha).hora, local(amanha).minuto), (9, 0));
        assert_eq!(Local { ano: 2026, mes: 11, dia: 2, hora: 9, minuto: 5 }.texto(), "02/11/2026 09:05");
    }

    #[test]
    fn interpreta_o_que_se_escreve() {
        // Segunda, 02/11/2026 às 14:00 no relógio local.
        let agora = de_local(2026, 11, 2, 14, 0);
        let ver = |t: &str| interpretar(t, agora).map(|t| local(t).texto());
        assert_eq!(ver("15/11 14:30").as_deref(), Some("15/11/2026 14:30"));
        assert_eq!(ver("15/11/27 9h").as_deref(), Some("15/11/2027 09:00"));
        assert_eq!(ver("15/11").as_deref(), Some("15/11/2026 09:00"));
        assert_eq!(ver("18:00").as_deref(), Some("02/11/2026 18:00"));
        assert_eq!(ver("8h").as_deref(), Some("03/11/2026 08:00"), "já passou: amanhã");
        assert_eq!(ver("Amanhã 8h30").as_deref(), Some("03/11/2026 08:30"));
        assert_eq!(ver("1/2").as_deref(), Some("01/02/2027 09:00"), "sem ano e já passou: ano que vem");
        assert_eq!(ver("01/11/2026 10:00"), None, "com ano e já passou");
        for ruim in ["", "31/02", "25:00", "abc", "10:75", "1/2/3/4"] {
            assert_eq!(ver(ruim), None, "{ruim}");
        }
        assert_eq!(descrever(de_local(2026, 11, 15, 14, 30)), "dom 15/11 às 14:30");
    }
}
