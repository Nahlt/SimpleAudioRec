//! Backend audio per Windows (WASAPI).
//!
//! - Microfono: cattura in modalità condivisa dal dispositivo scelto o dal
//!   predefinito (per comunicazioni o di sistema), seguendone i cambi.
//! - Output: loopback del dispositivo di riproduzione. Sullo stesso dispositivo
//!   viene riprodotto silenzio, così il loopback produce dati anche quando
//!   nessuno parla e l'orologio resta continuo.
//! - Applicazione: process loopback (Windows 10 2004+) sull'intero albero di
//!   processi dell'applicazione scelta.
//! - Mute: `IAudioEndpointVolume` su ogni dispositivo di acquisizione.

use simpleaudiorec_core::audio::{
    AppInfo, AudioBackend, CaptureControl, CaptureHandle, CaptureStatus, DeviceInfo, ThreadCapture, downmix,
};
use simpleaudiorec_core::fifo::SourceFifo;
use simpleaudiorec_core::settings::{SourceMode, SourceSetting};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{
    AUDCLNT_SHAREMODE_SHARED, AUDIOCLIENT_ACTIVATION_PARAMS, AUDIOCLIENT_ACTIVATION_PARAMS_0,
    AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK, AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS, ActivateAudioInterfaceAsync,
    DEVICE_STATE_ACTIVE, EDataFlow, ERole, IActivateAudioInterfaceAsyncOperation, IActivateAudioInterfaceCompletionHandler,
    IActivateAudioInterfaceCompletionHandler_Impl, IAudioCaptureClient, IAudioClient, IAudioRenderClient,
    IAudioSessionControl2, IAudioSessionManager2, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
    PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE, VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, WAVEFORMATEX,
    WAVEFORMATEXTENSIBLE, eCapture, eCommunications, eConsole, eRender,
};
use windows::Win32::System::Com::StructuredStorage::{PROPVARIANT, PropVariantClear, PropVariantToStringAlloc};
use windows::Win32::System::Com::{
    BLOB, CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree, STGM_READ,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    CreateEventW, GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, SetEvent, WaitForSingleObject,
};
use windows::Win32::System::Variant::VT_BLOB;
use windows::core::{GUID, HSTRING, Interface, PCWSTR, implement};

const STREAMFLAGS_LOOPBACK: u32 = 0x0002_0000;
const STREAMFLAGS_EVENTCALLBACK: u32 = 0x0004_0000;
const STREAMFLAGS_SRC_DEFAULT_QUALITY: u32 = 0x0800_0000;
const STREAMFLAGS_AUTOCONVERTPCM: u32 = 0x8000_0000;
const BUFFERFLAGS_SILENT: u32 = 0x2;
const BUFFER_100NS: i64 = 2_000_000; // 200 ms
const TARGET_RATE: u32 = 48_000;
const STILL_ACTIVE: u32 = 259;
const SUBTYPE_FLOAT: GUID = GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71);
const SUBTYPE_PCM: GUID = GUID::from_u128(0x00000001_0000_0010_8000_00aa00389b71);

fn com_init() {
    // Su un thread già inizializzato in STA (UI) restituisce un errore innocuo.
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
}

fn enumerator() -> windows::core::Result<IMMDeviceEnumerator> {
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
}

fn device_id(d: &IMMDevice) -> Option<String> {
    unsafe {
        let p = d.GetId().ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as _));
        s
    }
}

fn device_name(d: &IMMDevice) -> String {
    unsafe {
        let Ok(store) = d.OpenPropertyStore(STGM_READ) else { return String::new() };
        let Ok(mut pv) = store.GetValue(&PKEY_Device_FriendlyName) else { return String::new() };
        let name = match PropVariantToStringAlloc(&pv) {
            Ok(p) => {
                let s = p.to_string().unwrap_or_default();
                CoTaskMemFree(Some(p.0 as _));
                s
            }
            Err(_) => String::new(),
        };
        let _ = PropVariantClear(&mut pv);
        name
    }
}

fn active_devices(flow: EDataFlow) -> Vec<IMMDevice> {
    com_init();
    let mut out = Vec::new();
    unsafe {
        let Ok(en) = enumerator() else { return out };
        let Ok(col) = en.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE) else { return out };
        let n = col.GetCount().unwrap_or(0);
        for i in 0..n {
            if let Ok(d) = col.Item(i) {
                out.push(d);
            }
        }
    }
    out
}

fn list(flow: EDataFlow) -> Vec<DeviceInfo> {
    let mut v: Vec<DeviceInfo> = active_devices(flow)
        .iter()
        .filter_map(|d| Some(DeviceInfo { id: device_id(d)?, name: device_name(d) }))
        .collect();
    v.sort_by_key(|d| d.name.to_lowercase());
    v
}

fn role(mode: SourceMode) -> Option<ERole> {
    match mode {
        SourceMode::Communications => Some(eCommunications),
        SourceMode::System => Some(eConsole),
        _ => None,
    }
}

/// Dispositivo corrispondente all'impostazione, se presente e attivo.
fn resolve(flow: EDataFlow, sel: &SourceSetting) -> Option<IMMDevice> {
    com_init();
    let en = enumerator().ok()?;
    unsafe {
        if let Some(r) = role(sel.mode) {
            return en.GetDefaultAudioEndpoint(flow, r).ok();
        }
        let d = en.GetDevice(&HSTRING::from(sel.id.as_str())).ok()?;
        (d.GetState().ok()? == DEVICE_STATE_ACTIVE).then_some(d)
    }
}

/// Formato dei campioni restituiti da WASAPI.
#[derive(Clone, Copy)]
struct Fmt {
    channels: usize,
    rate: u32,
    float: bool,
    bytes: usize,
}

unsafe fn parse_format(p: *const WAVEFORMATEX) -> Option<Fmt> {
    let f = unsafe { *p };
    let tag = f.wFormatTag as u32;
    let float = match tag {
        3 => true,
        1 => false,
        0xFFFE => {
            let ext = unsafe { *(p as *const WAVEFORMATEXTENSIBLE) };
            let sub = ext.SubFormat;
            if sub == SUBTYPE_FLOAT {
                true
            } else if sub == SUBTYPE_PCM {
                false
            } else {
                return None;
            }
        }
        _ => return None,
    };
    let bytes = (f.wBitsPerSample / 8) as usize;
    if f.nChannels == 0 || !(float && bytes == 4 || !float && matches!(bytes, 2..=4)) {
        return None;
    }
    Some(Fmt { channels: f.nChannels as usize, rate: f.nSamplesPerSec, float, bytes })
}

fn float_format(channels: u16, rate: u32, mask: u32) -> WAVEFORMATEXTENSIBLE {
    let mut w = WAVEFORMATEXTENSIBLE::default();
    w.Format.wFormatTag = 0xFFFE;
    w.Format.nChannels = channels;
    w.Format.nSamplesPerSec = rate;
    w.Format.wBitsPerSample = 32;
    w.Format.nBlockAlign = channels * 4;
    w.Format.nAvgBytesPerSec = rate * channels as u32 * 4;
    w.Format.cbSize = 22;
    w.Samples.wValidBitsPerSample = 32;
    w.dwChannelMask = mask;
    w.SubFormat = SUBTYPE_FLOAT;
    w
}

/// Converte un pacchetto WASAPI in campioni f32 interleaved.
fn to_f32(data: *const u8, frames: usize, fmt: Fmt, out: &mut Vec<f32>) {
    let n = frames * fmt.channels;
    out.clear();
    out.reserve(n);
    let bytes = unsafe { std::slice::from_raw_parts(data, n * fmt.bytes) };
    match (fmt.float, fmt.bytes) {
        (true, _) => out.extend(bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))),
        (false, 2) => out.extend(bytes.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)),
        (false, 3) => out.extend(
            bytes.chunks_exact(3).map(|b| (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_608.0),
        ),
        _ => out.extend(
            bytes.chunks_exact(4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2_147_483_648.0),
        ),
    }
}

/// Inizializza un client di cattura: prima chiede float a 48 kHz (conversione
/// fatta da Windows), altrimenti usa il formato nativo del dispositivo.
unsafe fn init_capture_client(client: &IAudioClient, extra_flags: u32) -> windows::core::Result<Fmt> {
    unsafe {
        let mix = client.GetMixFormat()?;
        let native = parse_format(mix);
        let mask = if (*mix).wFormatTag == 0xFFFE { (*(mix as *const WAVEFORMATEXTENSIBLE)).dwChannelMask } else { 0 };
        let channels = (*mix).nChannels;
        CoTaskMemFree(Some(mix as _));
        let want = float_format(channels, TARGET_RATE, mask);
        let flags = extra_flags | STREAMFLAGS_AUTOCONVERTPCM | STREAMFLAGS_SRC_DEFAULT_QUALITY;
        if client
            .Initialize(AUDCLNT_SHAREMODE_SHARED, flags, BUFFER_100NS, 0, &want as *const _ as *const WAVEFORMATEX, None)
            .is_ok()
        {
            return Ok(Fmt { channels: channels as usize, rate: TARGET_RATE, float: true, bytes: 4 });
        }
        let mix = client.GetMixFormat()?;
        let res = client.Initialize(AUDCLNT_SHAREMODE_SHARED, extra_flags, BUFFER_100NS, 0, mix, None);
        CoTaskMemFree(Some(mix as _));
        res?;
        native.ok_or_else(|| windows::core::Error::from_hresult(windows::core::HRESULT(0x88890008u32 as i32)))
    }
}

/// Svuota la coda di cattura verso la FIFO. Restituisce `true` se ha letto dati.
unsafe fn drain(
    cap: &IAudioCaptureClient,
    fmt: Fmt,
    sink: &SourceFifo,
    tmp: &mut Vec<f32>,
    mono: &mut Vec<f32>,
) -> windows::core::Result<bool> {
    let mut got = false;
    unsafe {
        while cap.GetNextPacketSize()? > 0 {
            let mut data = std::ptr::null_mut();
            let mut frames = 0u32;
            let mut flags = 0u32;
            cap.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
            if frames > 0 {
                if flags & BUFFERFLAGS_SILENT != 0 || data.is_null() {
                    mono.clear();
                    mono.resize(frames as usize, 0.0);
                } else {
                    to_f32(data, frames as usize, fmt, tmp);
                    downmix(tmp, fmt.channels, mono);
                }
                sink.push(mono, fmt.rate);
                got = true;
            }
            cap.ReleaseBuffer(frames)?;
        }
    }
    Ok(got)
}

/// Client che riproduce silenzio sul dispositivo di loopback.
struct SilentRender {
    client: IAudioClient,
    render: IAudioRenderClient,
    size: u32,
}

impl SilentRender {
    unsafe fn new(dev: &IMMDevice) -> windows::core::Result<Self> {
        unsafe {
            let client: IAudioClient = dev.Activate(CLSCTX_ALL, None)?;
            let mix = client.GetMixFormat()?;
            let r = client.Initialize(AUDCLNT_SHAREMODE_SHARED, 0, BUFFER_100NS, 0, mix, None);
            CoTaskMemFree(Some(mix as _));
            r?;
            let render: IAudioRenderClient = client.GetService()?;
            let size = client.GetBufferSize()?;
            let s = Self { client, render, size };
            s.feed()?;
            s.client.Start()?;
            Ok(s)
        }
    }

    unsafe fn feed(&self) -> windows::core::Result<()> {
        unsafe {
            let avail = self.size - self.client.GetCurrentPadding()?;
            if avail > 0 {
                self.render.GetBuffer(avail)?;
                self.render.ReleaseBuffer(avail, BUFFERFLAGS_SILENT)?;
            }
        }
        Ok(())
    }
}

impl Drop for SilentRender {
    fn drop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
        }
    }
}

enum Exit {
    Stopped,
    Restart,
}

/// Cattura da un dispositivo finché non viene fermata o il dispositivo cambia.
fn run_device_stream(
    dev: &IMMDevice,
    flow: EDataFlow,
    sel: &SourceSetting,
    ctl: &CaptureControl,
    sink: &SourceFifo,
) -> windows::core::Result<Exit> {
    let loopback = flow == eRender;
    let my_id = device_id(dev);
    unsafe {
        let client: IAudioClient = dev.Activate(CLSCTX_ALL, None)?;
        let fmt = init_capture_client(&client, if loopback { STREAMFLAGS_LOOPBACK } else { 0 })?;
        let cap: IAudioCaptureClient = client.GetService()?;
        let silent = if loopback { SilentRender::new(dev).ok() } else { None };
        client.Start()?;
        ctl.set_status(CaptureStatus::Running);
        let (mut tmp, mut mono) = (Vec::new(), Vec::new());
        let mut last_check = Instant::now();
        let result = loop {
            if ctl.should_stop() {
                break Ok(Exit::Stopped);
            }
            std::thread::sleep(Duration::from_millis(10));
            if let Some(s) = &silent {
                let _ = s.feed();
            }
            if let Err(e) = drain(&cap, fmt, sink, &mut tmp, &mut mono) {
                break Err(e);
            }
            if last_check.elapsed() >= Duration::from_millis(500) {
                last_check = Instant::now();
                if role(sel.mode).is_some() && resolve(flow, sel).and_then(|d| device_id(&d)) != my_id {
                    break Ok(Exit::Restart);
                }
            }
        };
        let _ = client.Stop();
        result
    }
}

fn device_capture(flow: EDataFlow, sel: SourceSetting, sink: Arc<SourceFifo>) -> ThreadCapture {
    let name = if flow == eRender { "loopback" } else { "microfono" };
    ThreadCapture::spawn(name, move |ctl| {
        com_init();
        while !ctl.should_stop() {
            match resolve(flow, &sel) {
                None => {
                    ctl.set_status(CaptureStatus::DeviceMissing);
                    std::thread::sleep(Duration::from_millis(500));
                }
                Some(dev) => match run_device_stream(&dev, flow, &sel, &ctl, &sink) {
                    Ok(Exit::Stopped) => break,
                    Ok(Exit::Restart) => {}
                    Err(e) => {
                        log::warn!("cattura {name}: {e}");
                        ctl.set_status(CaptureStatus::DeviceMissing);
                        std::thread::sleep(Duration::from_millis(500));
                    }
                },
            }
        }
    })
}

// ---------- Cattura per applicazione (process loopback) ----------

#[implement(IActivateAudioInterfaceCompletionHandler)]
struct ActivationHandler {
    event: HANDLE,
}

impl IActivateAudioInterfaceCompletionHandler_Impl for ActivationHandler_Impl {
    fn ActivateCompleted(
        &self,
        _op: windows::core::Ref<IActivateAudioInterfaceAsyncOperation>,
    ) -> windows::core::Result<()> {
        unsafe { SetEvent(self.event) }
    }
}

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

unsafe fn activate_process_loopback(pid: u32) -> windows::core::Result<IAudioClient> {
    unsafe {
        let event = OwnedHandle(CreateEventW(None, false, false, PCWSTR::null())?);
        let mut params = AUDIOCLIENT_ACTIVATION_PARAMS {
            ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
            Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
                ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                    TargetProcessId: pid,
                    ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
                },
            },
        };
        let mut pv = PROPVARIANT::default();
        {
            let inner = &mut *pv.Anonymous.Anonymous;
            inner.vt = VT_BLOB;
            inner.Anonymous.blob = BLOB {
                cbSize: std::mem::size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
                pBlobData: &mut params as *mut _ as *mut u8,
            };
        }
        let handler: IActivateAudioInterfaceCompletionHandler = ActivationHandler { event: event.0 }.into();
        let op = ActivateAudioInterfaceAsync(
            VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
            &IAudioClient::IID,
            Some(&pv),
            &handler,
        )?;
        if WaitForSingleObject(event.0, 5_000) != WAIT_OBJECT_0 {
            return Err(windows::core::Error::from_hresult(windows::core::HRESULT(0x8000_000Eu32 as i32)));
        }
        let mut hr = windows::core::HRESULT(0);
        let mut unk = None;
        op.GetActivateResult(&mut hr, &mut unk)?;
        hr.ok()?;
        unk.ok_or_else(|| windows::core::Error::from_hresult(windows::core::HRESULT(0x8000_4005u32 as i32)))?.cast()
    }
}

fn process_alive(pid: u32) -> bool {
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return false };
        let h = OwnedHandle(h);
        let mut code = 0u32;
        GetExitCodeProcess(h.0, &mut code).is_ok() && code == STILL_ACTIVE
    }
}

fn run_app_stream(pid: u32, ctl: &CaptureControl, sink: &SourceFifo) -> windows::core::Result<()> {
    unsafe {
        let client = activate_process_loopback(pid)?;
        // Il process loopback non fornisce il formato: va indicato esplicitamente.
        let fmt = float_format(2, TARGET_RATE, 0x3);
        client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            STREAMFLAGS_LOOPBACK | STREAMFLAGS_EVENTCALLBACK | STREAMFLAGS_AUTOCONVERTPCM,
            BUFFER_100NS,
            0,
            &fmt as *const _ as *const WAVEFORMATEX,
            None,
        )?;
        let event = OwnedHandle(CreateEventW(None, false, false, PCWSTR::null())?);
        client.SetEventHandle(event.0)?;
        let cap: IAudioCaptureClient = client.GetService()?;
        client.Start()?;
        ctl.set_status(CaptureStatus::Running);
        let f = Fmt { channels: 2, rate: TARGET_RATE, float: true, bytes: 4 };
        let (mut tmp, mut mono) = (Vec::new(), Vec::new());
        let mut last_check = Instant::now();
        let r = loop {
            if ctl.should_stop() {
                break Ok(());
            }
            WaitForSingleObject(event.0, 50);
            if let Err(e) = drain(&cap, f, sink, &mut tmp, &mut mono) {
                break Err(e);
            }
            if last_check.elapsed() >= Duration::from_secs(1) {
                last_check = Instant::now();
                if !process_alive(pid) {
                    break Ok(());
                }
            }
        };
        let _ = client.Stop();
        r
    }
}

fn app_capture(exe: String, sink: Arc<SourceFifo>) -> ThreadCapture {
    ThreadCapture::spawn("app-loopback", move |ctl| {
        com_init();
        while !ctl.should_stop() {
            let Some(pid) = find_root_process(&exe) else {
                ctl.set_status(CaptureStatus::DeviceMissing);
                std::thread::sleep(Duration::from_millis(1000));
                continue;
            };
            if let Err(e) = run_app_stream(pid, &ctl, &sink) {
                log::warn!("cattura applicazione {exe}: {e}");
            }
            if !ctl.should_stop() {
                ctl.set_status(CaptureStatus::DeviceMissing);
                std::thread::sleep(Duration::from_millis(1000));
            }
        }
    })
}

// ---------- Processi ----------

struct ProcInfo {
    parent: u32,
    exe: String,
}

fn snapshot() -> HashMap<u32, ProcInfo> {
    let mut map = HashMap::new();
    unsafe {
        let Ok(h) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return map };
        let h = OwnedHandle(h);
        let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(h.0, &mut e).is_ok();
        while ok {
            let len = e.szExeFile.iter().position(|c| *c == 0).unwrap_or(e.szExeFile.len());
            let exe = String::from_utf16_lossy(&e.szExeFile[..len]);
            map.insert(e.th32ProcessID, ProcInfo { parent: e.th32ParentProcessID, exe });
            ok = Process32NextW(h.0, &mut e).is_ok();
        }
    }
    map
}

const SYSTEM_PARENTS: [&str; 8] = [
    "explorer.exe",
    "svchost.exe",
    "services.exe",
    "wininit.exe",
    "winlogon.exe",
    "sihost.exe",
    "runtimebroker.exe",
    "system",
];

/// Risale ai processi padre fino all'applicazione "principale" (es. da
/// `msedgewebview2.exe` a `ms-teams.exe`), fermandosi ai processi di sistema.
fn root_app(pid: u32, procs: &HashMap<u32, ProcInfo>) -> Option<u32> {
    let mut cur = pid;
    procs.get(&cur)?;
    for _ in 0..8 {
        let Some(p) = procs.get(&cur) else { break };
        match procs.get(&p.parent) {
            Some(parent) if p.parent != 0 && !SYSTEM_PARENTS.contains(&parent.exe.to_lowercase().as_str()) => {
                cur = p.parent
            }
            _ => break,
        }
    }
    Some(cur)
}

/// Processo radice con l'eseguibile indicato (quello il cui padre ha un altro nome).
fn find_root_process(exe: &str) -> Option<u32> {
    let procs = snapshot();
    let mut candidates: Vec<u32> = procs
        .iter()
        .filter(|(_, p)| p.exe.eq_ignore_ascii_case(exe))
        .filter(|(_, p)| procs.get(&p.parent).is_none_or(|pp| !pp.exe.eq_ignore_ascii_case(exe)))
        .map(|(pid, _)| *pid)
        .collect();
    candidates.sort();
    candidates.first().copied()
}

// ---------- Backend ----------

pub struct WasapiBackend;

impl AudioBackend for WasapiBackend {
    fn input_devices(&self) -> Vec<DeviceInfo> {
        list(eCapture)
    }

    fn output_devices(&self) -> Vec<DeviceInfo> {
        list(eRender)
    }

    fn audio_apps(&self) -> Vec<AppInfo> {
        let procs = snapshot();
        let me = std::process::id();
        let mut out: Vec<AppInfo> = Vec::new();
        for dev in active_devices(eRender) {
            unsafe {
                let Ok(mgr) = dev.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else { continue };
                let Ok(en) = mgr.GetSessionEnumerator() else { continue };
                for i in 0..en.GetCount().unwrap_or(0) {
                    let Some(pid) = en
                        .GetSession(i)
                        .ok()
                        .and_then(|s| s.cast::<IAudioSessionControl2>().ok())
                        .and_then(|s| s.GetProcessId().ok())
                    else {
                        continue;
                    };
                    if pid == 0 || pid == me {
                        continue;
                    }
                    let Some(root) = root_app(pid, &procs) else { continue };
                    let exe = procs[&root].exe.clone();
                    if !out.iter().any(|a| a.exe.eq_ignore_ascii_case(&exe)) {
                        out.push(AppInfo { pid: root, exe });
                    }
                }
            }
        }
        out.sort_by_key(|a| a.exe.to_lowercase());
        out
    }

    fn resolve_input_id(&self, sel: &SourceSetting) -> Option<String> {
        resolve(eCapture, sel).and_then(|d| device_id(&d))
    }

    fn capture_mute_states(&self) -> Vec<(String, bool)> {
        active_devices(eCapture)
            .iter()
            .filter_map(|d| unsafe {
                let vol: IAudioEndpointVolume = d.Activate(CLSCTX_ALL, None).ok()?;
                Some((device_id(d)?, vol.GetMute().ok()?.as_bool()))
            })
            .collect()
    }

    fn set_capture_mute(&self, id: &str, mute: bool) -> bool {
        com_init();
        unsafe {
            let Ok(en) = enumerator() else { return false };
            let Ok(d) = en.GetDevice(&HSTRING::from(id)) else { return false };
            let Ok(vol) = d.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) else { return false };
            vol.SetMute(mute, std::ptr::null()).is_ok()
        }
    }

    fn start_input(&self, sel: &SourceSetting, sink: Arc<SourceFifo>) -> Box<dyn CaptureHandle> {
        Box::new(device_capture(eCapture, sel.clone(), sink))
    }

    fn start_output(&self, sel: &SourceSetting, sink: Arc<SourceFifo>) -> Box<dyn CaptureHandle> {
        if sel.mode == SourceMode::App {
            Box::new(app_capture(sel.name.clone(), sink))
        } else {
            Box::new(device_capture(eRender, sel.clone(), sink))
        }
    }
}
