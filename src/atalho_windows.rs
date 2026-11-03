//! Win+Alt+N no Windows: o mesmo atalho de post-it que o GNOME tem pelo gsettings. Vale enquanto o
//! FeatherNotes está aberto (ele fica em segundo plano por causa dos post-its).
use pelicano::futures;
use std::sync::atomic::{AtomicU32, Ordering};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_NOREPEAT, MOD_WIN};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetMessageW, PostThreadMessageW, MSG, WM_HOTKEY, WM_QUIT};

/// A thread que escuta o atalho (0 = nenhuma).
static FIO: AtomicU32 = AtomicU32::new(0);

/// Registra o atalho; cada aperto vira um item do fluxo. Erro se outro programa já usa a combinação.
pub fn ligar() -> std::io::Result<impl futures::Stream<Item = ()> + Send> {
    desligar();
    let (envia, recebe) = futures::channel::mpsc::unbounded();
    let (pronto, resposta) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        // SAFETY: atalho da própria thread (janela nula); a fila de mensagens é dela.
        unsafe {
            if RegisterHotKey(std::ptr::null_mut(), 1, MOD_WIN | MOD_ALT | MOD_NOREPEAT, u32::from(b'N')) == 0 {
                let _ = pronto.send(Err(std::io::Error::last_os_error()));
                return;
            }
            FIO.store(GetCurrentThreadId(), Ordering::SeqCst);
            let _ = pronto.send(Ok(()));
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                if msg.message == WM_HOTKEY && envia.unbounded_send(()).is_err() {
                    break;
                }
            }
            UnregisterHotKey(std::ptr::null_mut(), 1);
        }
    });
    resposta.recv().unwrap_or_else(|_| Err(std::io::Error::other("a thread do atalho não respondeu")))?;
    Ok(recebe)
}

/// Solta o atalho (a thread sai do laço e o fluxo acaba).
pub fn desligar() {
    let fio = FIO.swap(0, Ordering::SeqCst);
    if fio != 0 {
        // SAFETY: só posta uma mensagem na fila da thread do atalho.
        unsafe { PostThreadMessageW(fio, WM_QUIT, 0, 0) };
    }
}
