//! Funzioni specifiche di Windows: istanza singola, spazio su disco, overlay
//! sull'icona della barra delle applicazioni, controllo della posizione.

#[cfg(windows)]
mod imp {
    use std::path::Path;
    use windows::Win32::Foundation::{
        CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HWND, POINT, WAIT_OBJECT_0,
    };
    use windows::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject, MONITOR_DEFAULTTONULL, MonitorFromPoint};
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx};
    use windows::Win32::System::Threading::{
        CreateEventW, CreateMutexW, EVENT_MODIFY_STATE, INFINITE, OpenEventW, SetEvent, WaitForSingleObject,
    };
    use windows::Win32::UI::Shell::{ITaskbarList3, TaskbarList};
    use windows::Win32::UI::WindowsAndMessaging::{
        ASFW_ANY, AllowSetForegroundWindow, CreateIconIndirect, DestroyIcon, HICON, ICONINFO,
    };
    use windows::core::{HSTRING, w};

    struct SendHandle(HANDLE);
    unsafe impl Send for SendHandle {}

    pub struct SingleInstance {
        _mutex: SendHandle,
        event: SendHandle,
    }

    /// `None` se un'altra istanza è già in esecuzione (che viene portata in primo piano).
    pub fn single_instance() -> Option<SingleInstance> {
        unsafe {
            let mutex = CreateMutexW(None, true, w!("Local\\SimpleAudioRec.SingleInstance")).ok()?;
            if GetLastError() == ERROR_ALREADY_EXISTS {
                if let Ok(ev) = OpenEventW(EVENT_MODIFY_STATE, false, w!("Local\\SimpleAudioRec.Show")) {
                    let _ = AllowSetForegroundWindow(ASFW_ANY);
                    let _ = SetEvent(ev);
                    let _ = CloseHandle(ev);
                }
                let _ = CloseHandle(mutex);
                return None;
            }
            let event = CreateEventW(None, false, false, w!("Local\\SimpleAudioRec.Show")).ok()?;
            Some(SingleInstance { _mutex: SendHandle(mutex), event: SendHandle(event) })
        }
    }

    impl SingleInstance {
        /// Chiama `f` ogni volta che un'altra istanza chiede di mostrare la finestra.
        pub fn listen(self, f: impl Fn() + Send + 'static) {
            std::thread::spawn(move || {
                let me = self;
                loop {
                    if unsafe { WaitForSingleObject(me.event.0, INFINITE) } != WAIT_OBJECT_0 {
                        break;
                    }
                    f();
                }
            });
        }
    }

    pub fn free_disk_bytes(path: &Path) -> Option<u64> {
        let mut free = 0u64;
        unsafe { GetDiskFreeSpaceExW(&HSTRING::from(path.as_os_str()), Some(&mut free), None, None).ok()? };
        Some(free)
    }

    pub fn point_on_screen(x: f32, y: f32) -> bool {
        let p = POINT { x: x as i32 + 40, y: y as i32 + 10 };
        !unsafe { MonitorFromPoint(p, MONITOR_DEFAULTTONULL) }.is_invalid()
    }

    /// Overlay sull'icona della barra delle applicazioni (pallino colorato).
    pub struct Taskbar {
        list: ITaskbarList3,
        hwnd: HWND,
        icon: Option<HICON>,
        current: Option<[u8; 3]>,
    }

    fn dot_icon(rgb: [u8; 3]) -> Option<HICON> {
        const N: i32 = 16;
        let mut bgra = vec![0u8; (N * N * 4) as usize];
        let c = (N as f32 - 1.0) / 2.0;
        let r = N as f32 / 2.0 - 1.0;
        for y in 0..N {
            for x in 0..N {
                let d = ((x as f32 - c).powi(2) + (y as f32 - c).powi(2)).sqrt();
                let a = (r - d + 0.5).clamp(0.0, 1.0);
                let i = ((y * N + x) * 4) as usize;
                // alpha premoltiplicato
                bgra[i] = (rgb[2] as f32 * a) as u8;
                bgra[i + 1] = (rgb[1] as f32 * a) as u8;
                bgra[i + 2] = (rgb[0] as f32 * a) as u8;
                bgra[i + 3] = (a * 255.0) as u8;
            }
        }
        let mask = vec![0u8; (N * N / 8) as usize];
        unsafe {
            let color = CreateBitmap(N, N, 1, 32, Some(bgra.as_ptr() as _));
            let maskb = CreateBitmap(N, N, 1, 1, Some(mask.as_ptr() as _));
            let info = ICONINFO { fIcon: true.into(), xHotspot: 0, yHotspot: 0, hbmMask: maskb, hbmColor: color };
            let icon = CreateIconIndirect(&info).ok();
            let _ = DeleteObject(color.into());
            let _ = DeleteObject(maskb.into());
            icon
        }
    }

    impl Taskbar {
        pub fn new(hwnd: isize) -> Option<Self> {
            unsafe {
                // Il thread dell'interfaccia è normalmente già inizializzato (STA).
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                let list: ITaskbarList3 = CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER).ok()?;
                list.HrInit().ok()?;
                Some(Self { list, hwnd: HWND(hwnd as _), icon: None, current: None })
            }
        }

        /// Mostra un pallino del colore indicato, o lo rimuove con `None`.
        pub fn set(&mut self, rgb: Option<[u8; 3]>, label: &str) {
            if rgb == self.current {
                return;
            }
            self.current = rgb;
            unsafe {
                let new = rgb.and_then(dot_icon);
                let _ = self.list.SetOverlayIcon(self.hwnd, new.unwrap_or_default(), &HSTRING::from(label));
                if let Some(old) = self.icon.take() {
                    let _ = DestroyIcon(old);
                }
                self.icon = new;
            }
        }
    }

    impl Drop for Taskbar {
        fn drop(&mut self) {
            if let Some(i) = self.icon.take() {
                unsafe {
                    let _ = DestroyIcon(i);
                }
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::path::Path;

    pub struct SingleInstance;

    pub fn single_instance() -> Option<SingleInstance> {
        Some(SingleInstance)
    }

    impl SingleInstance {
        pub fn listen(self, _f: impl Fn() + Send + 'static) {}
    }

    pub fn free_disk_bytes(_path: &Path) -> Option<u64> {
        None
    }

    pub fn point_on_screen(_x: f32, _y: f32) -> bool {
        true
    }

    pub struct Taskbar;

    impl Taskbar {
        pub fn new(_hwnd: isize) -> Option<Self> {
            None
        }
        pub fn set(&mut self, _rgb: Option<[u8; 3]>, _label: &str) {}
    }
}

pub use imp::*;
