//! Applicazione: stato, flussi (registrazione, salvataggio, recupero) e finestre.

use crate::hotkeys::{HotkeyService, combo_from};
use crate::mute_worker::MuteWorker;
use crate::platform;
use crate::ui::{self, GREEN, Icon, ORANGE, RED};
use chrono::Local;
use egui::{RichText, ViewportCommand, vec2};
use simpleaudiorec_core::audio::{AppInfo, AudioBackend, CaptureStatus, DeviceInfo};
use simpleaudiorec_core::export::{self, ExportSpec};
use simpleaudiorec_core::i18n::{self, Strings, fill};
use simpleaudiorec_core::naming::{self, OutputPaths};
use simpleaudiorec_core::paths::DataPaths;
use simpleaudiorec_core::recorder::{Finished, Recorder};
use simpleaudiorec_core::session::{SessionDir, SessionMeta, Unsaved, find_unsaved};
use simpleaudiorec_core::settings::{Action, BITRATES, Format, Lang, Settings, SourceMode, SourceSetting};
use simpleaudiorec_core::{APP_NAME, format_hms};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

const MAIN_SIZE: [f32; 2] = [420.0, 250.0];
const SAVE_SIZE: [f32; 2] = [440.0, 340.0];
const SETTINGS_SIZE: [f32; 2] = [520.0, 600.0];
const DISK_LOW_BYTES: u64 = 1_000_000_000;

struct SaveState {
    finished: Finished,
    both: bool,
    labels: Vec<String>,
}

struct ExportJob {
    progress: Arc<AtomicU32>,
    rx: Receiver<Result<Vec<PathBuf>, String>>,
    save: SaveState,
    dir: Option<PathBuf>,
}

enum Phase {
    Idle,
    Recording(Box<Recorder>),
    Save(SaveState),
    Exporting(ExportJob),
}

enum Dialog {
    CloseWhileRecording,
    ConfirmDiscard,
    Recovery(Unsaved),
    ConfirmDiscardRecovered(Unsaved),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Main,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Level {
    Info,
    Warn,
}

struct Notice {
    text: String,
    level: Level,
}

pub struct SimpleAudioRecApp {
    paths: DataPaths,
    settings: Settings,
    system_locale: Option<String>,
    backend: Arc<dyn AudioBackend>,
    mute: MuteWorker,
    hotkeys: HotkeyService,
    phase: Phase,
    view: View,
    dialog: Option<Dialog>,
    notices: Vec<Notice>,
    recovery: Vec<Unsaved>,
    inputs: Vec<DeviceInfo>,
    outputs: Vec<DeviceInfo>,
    apps: Vec<AppInfo>,
    capturing_hotkey: Option<Action>,
    taskbar: Option<platform::Taskbar>,
    last_title: String,
    last_size: [f32; 2],
    window_pos: Option<[f32; 2]>,
    last_disk_check: Instant,
    disk_warned: bool,
    bookmark_flash: Option<Instant>,
    allow_close: bool,
}

impl SimpleAudioRecApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        paths: DataPaths,
        settings: Settings,
        settings_error: Option<String>,
        backend: Arc<dyn AudioBackend>,
        single: Option<platform::SingleInstance>,
    ) -> Self {
        let ctx = cc.egui_ctx.clone();
        let mut style = (*ctx.global_style()).clone();
        style.spacing.item_spacing = vec2(8.0, 8.0);
        style.spacing.button_padding = vec2(10.0, 5.0);
        ctx.set_global_style(style);

        if let Some(s) = single {
            let c = ctx.clone();
            s.listen(move || {
                c.send_viewport_cmd(ViewportCommand::Visible(true));
                c.send_viewport_cmd(ViewportCommand::Minimized(false));
                c.send_viewport_cmd(ViewportCommand::Focus);
                c.request_repaint();
            });
        }

        let c = ctx.clone();
        let mute = MuteWorker::start(backend.clone(), paths.mute_owner_file(), settings.input.clone(), move || {
            c.request_repaint()
        });
        let hotkeys = HotkeyService::new(ctx.clone());

        let taskbar = {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            match cc.window_handle().map(|h| h.as_raw()) {
                Ok(RawWindowHandle::Win32(h)) => platform::Taskbar::new(h.hwnd.get()),
                _ => None,
            }
        };

        let mut app = Self {
            recovery: find_unsaved(&paths.recovery_dir()),
            paths,
            settings,
            system_locale: sys_locale::get_locale(),
            backend,
            mute,
            hotkeys,
            phase: Phase::Idle,
            view: View::Main,
            dialog: None,
            notices: Vec::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            apps: Vec::new(),
            capturing_hotkey: None,
            taskbar,
            last_title: String::new(),
            last_size: MAIN_SIZE,
            window_pos: None,
            last_disk_check: Instant::now(),
            disk_warned: false,
            bookmark_flash: None,
            allow_close: false,
        };
        if let Some(e) = settings_error {
            let msg = fill(app.t().settings_error, &[("err", &e)]);
            app.notify(msg, Level::Warn);
        }
        app.register_hotkeys();
        app.next_recovery();
        app
    }

    fn t(&self) -> &'static Strings {
        i18n::strings(i18n::resolve(self.settings.language, self.system_locale.as_deref()))
    }

    fn notify(&mut self, text: String, level: Level) {
        self.notices.retain(|n| n.text != text);
        self.notices.push(Notice { text, level });
        if self.notices.len() > 3 {
            self.notices.remove(0);
        }
    }

    fn save_settings(&mut self) {
        if let Err(e) = self.settings.save(&self.paths.settings_file()) {
            log::warn!("salvataggio impostazioni: {e}");
        }
    }

    fn register_hotkeys(&mut self) {
        let failed = self.hotkeys.register(&self.settings.hotkeys);
        for k in failed {
            let msg = fill(self.t().hk_busy, &[("key", &k)]);
            self.notify(msg, Level::Warn);
        }
    }

    fn next_recovery(&mut self) {
        if self.dialog.is_none()
            && matches!(self.phase, Phase::Idle)
            && let Some(u) = (!self.recovery.is_empty()).then(|| self.recovery.remove(0))
        {
            self.dialog = Some(Dialog::Recovery(u));
        }
    }

    // ---------- azioni ----------

    fn start(&mut self, ctx: &egui::Context) {
        if !matches!(self.phase, Phase::Idle) || self.dialog.is_some() {
            return;
        }
        let now = Local::now();
        let res = SessionDir::create(&self.paths.recovery_dir(), now).and_then(|dir| {
            Recorder::start(
                self.backend.as_ref(),
                &self.settings.input,
                &self.settings.output,
                dir,
                SessionMeta::new(now),
                self.mute.is_muted(),
            )
        });
        match res {
            Ok(r) => {
                self.phase = Phase::Recording(Box::new(r));
                self.view = View::Main;
                self.disk_warned = false;
                self.last_disk_check = Instant::now() - Duration::from_secs(60);
            }
            Err(e) => {
                let msg = fill(self.t().start_error, &[("err", &e.to_string())]);
                self.notify(msg, Level::Warn);
            }
        }
        ctx.request_repaint();
    }

    fn toggle_pause(&mut self) {
        if let Phase::Recording(r) = &self.phase {
            r.set_paused(!r.is_paused());
        }
    }

    fn bookmark(&mut self) {
        let label = self.t().bookmark_default;
        if let Phase::Recording(r) = &mut self.phase {
            r.add_bookmark(|n| fill(label, &[("n", &n.to_string())]));
            self.bookmark_flash = Some(Instant::now());
        }
    }

    fn stop(&mut self, ctx: &egui::Context) {
        let Phase::Recording(_) = &self.phase else { return };
        let Phase::Recording(r) = std::mem::replace(&mut self.phase, Phase::Idle) else { unreachable!() };
        match r.stop() {
            Ok(finished) => {
                let save = SaveState {
                    labels: finished.meta.bookmarks.iter().map(|b| b.label.clone()).collect(),
                    both: self.settings.both_formats,
                    finished,
                };
                if self.settings.autosave {
                    let dir = self.settings.last_dir.clone().unwrap_or_else(default_dir);
                    let out = self.auto_outputs(&save, &dir);
                    self.start_export(save, out, None);
                } else {
                    self.phase = Phase::Save(save);
                    ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(ViewportCommand::Focus);
                }
            }
            Err(e) => {
                let msg = fill(self.t().save_error, &[("err", &e.to_string())]);
                self.notify(msg, Level::Warn);
                // La sessione resta su disco e sarà proposta per il recupero.
                self.recovery = find_unsaved(&self.paths.recovery_dir());
                self.next_recovery();
            }
        }
    }

    fn base_name(save: &SaveState) -> String {
        save.finished
            .meta
            .start_time()
            .map(|t| naming::base_name(&t))
            .unwrap_or_else(|| naming::base_name(&Local::now()))
    }

    fn auto_outputs(&self, save: &SaveState, dir: &std::path::Path) -> OutputPaths {
        naming::outputs_auto(
            dir,
            &Self::base_name(save),
            self.settings.format,
            save.both,
            self.t().bookmarks_suffix,
            !save.labels.is_empty(),
        )
    }

    fn ask_save_path(&mut self) {
        let Phase::Save(save) = &self.phase else { return };
        let dir = self.settings.last_dir.clone().filter(|d| d.is_dir()).unwrap_or_else(default_dir);
        let name = naming::file_name(&Self::base_name(save), self.settings.format);
        let t = self.t();
        let chosen = rfd::FileDialog::new()
            .set_title(t.save_dialog_title)
            .set_directory(&dir)
            .set_file_name(&name)
            .add_filter(t.mp3_filter, &["mp3"])
            .save_file();
        match chosen {
            Some(path) => {
                let Phase::Save(save) = std::mem::replace(&mut self.phase, Phase::Idle) else { return };
                let out = naming::outputs_from_chosen(
                    &path,
                    self.settings.format,
                    save.both,
                    t.bookmarks_suffix,
                    !save.labels.is_empty(),
                );
                let parent = path.parent().map(|p| p.to_path_buf());
                self.start_export(save, out, parent);
            }
            None => self.dialog = Some(Dialog::ConfirmDiscard),
        }
    }

    fn start_export(&mut self, mut save: SaveState, outputs: OutputPaths, dir: Option<PathBuf>) {
        let fin = &mut save.finished;
        for (b, l) in fin.meta.bookmarks.iter_mut().zip(&save.labels) {
            let l = l.trim();
            if !l.is_empty() {
                b.label = l.to_string();
            }
        }
        let _ = fin.session.save_meta(&fin.meta);
        let start = fin.meta.start_time();
        let date = start.map(|t| t.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_default();
        let spec = ExportSpec {
            wav: fin.session.audio_path(),
            outputs,
            bitrate_kbps: self.settings.bitrate_kbps,
            normalize: self.settings.normalize,
            title: fill(self.t().recording_title, &[("date", &date)]),
            started_at: start,
            bookmarks: fin.meta.bookmarks.clone(),
        };
        let progress = Arc::new(AtomicU32::new(0));
        let (tx, rx) = channel();
        let p = progress.clone();
        std::thread::Builder::new()
            .name("export".into())
            .spawn(move || {
                let _ = tx.send(export::run(&spec, &p));
            })
            .expect("thread export");
        let dir = dir.or_else(|| self.settings.last_dir.clone());
        self.phase = Phase::Exporting(ExportJob { progress, rx, save, dir });
    }

    fn poll_export(&mut self) {
        let Phase::Exporting(job) = &self.phase else { return };
        let Ok(res) = job.rx.try_recv() else { return };
        let Phase::Exporting(job) = std::mem::replace(&mut self.phase, Phase::Idle) else { return };
        match res {
            Ok(files) => {
                let _ = job.save.finished.session.discard();
                if let Some(d) = job.dir {
                    self.settings.last_dir = Some(d);
                    self.save_settings();
                }
                let shown = files.first().map(|p| p.display().to_string()).unwrap_or_default();
                let msg = fill(self.t().saved_to, &[("path", &shown)]);
                self.notify(msg, Level::Info);
                self.next_recovery();
            }
            Err(e) => {
                let msg = fill(self.t().save_error, &[("err", &e)]);
                self.notify(msg, Level::Warn);
                self.phase = Phase::Save(job.save);
            }
        }
    }

    fn discard_current(&mut self) {
        if let Phase::Save(save) = std::mem::replace(&mut self.phase, Phase::Idle) {
            let _ = save.finished.session.discard();
        }
        self.next_recovery();
    }

    fn handle_action(&mut self, a: Action, ctx: &egui::Context) {
        match a {
            Action::Start => self.start(ctx),
            Action::Pause => self.toggle_pause(),
            Action::Stop => self.stop(ctx),
            Action::Mute => self.mute.toggle(),
            Action::Bookmark => self.bookmark(),
        }
    }

    fn refresh_lists(&mut self) {
        self.inputs = self.backend.input_devices();
        self.outputs = self.backend.output_devices();
        self.apps = self.backend.audio_apps();
    }

    fn set_view(&mut self, v: View) {
        if v == View::Settings {
            self.refresh_lists();
        }
        if v == View::Main && self.capturing_hotkey.take().is_some() {
            self.register_hotkeys();
        }
        self.view = v;
    }

    // ---------- finestre ----------

    fn status_header(&mut self, ui: &mut egui::Ui) {
        let t = self.t();
        let (dot, label, frames, bookmarks) = match &self.phase {
            Phase::Recording(r) if r.is_paused() => (ui::dim(ui), t.paused, r.shared.frames(), r.meta.bookmarks.len()),
            Phase::Recording(r) => (RED, t.rec, r.shared.frames(), r.meta.bookmarks.len()),
            Phase::Save(s) => (ui::dim(ui), t.ready, s.finished.frames, s.labels.len()),
            _ => (ui::dim(ui), t.ready, 0, 0),
        };
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(18.0), egui::Sense::hover());
            ui.painter().circle_filled(rect.center(), 7.0, dot);
            ui.label(RichText::new(label).strong().size(15.0));
            ui.add_space(6.0);
            ui.label(RichText::new(format_hms(frames)).monospace().size(22.0));
            if bookmarks > 0 {
                ui.add_space(6.0);
                let flash = self.bookmark_flash.is_some_and(|f| f.elapsed() < Duration::from_millis(600));
                let col = if flash { ORANGE } else { ui.visuals().text_color() };
                let (r, _) = ui.allocate_exact_size(egui::Vec2::splat(16.0), egui::Sense::hover());
                ui::paint_icon(ui.painter(), Icon::Bookmark, r, col);
                ui.label(RichText::new(bookmarks.to_string()).color(col));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui::small_icon_button(ui, Icon::Gear, t.settings).clicked() {
                    self.set_view(View::Settings);
                }
            });
        });
    }

    fn main_view(&mut self, ui: &mut egui::Ui) {
        let t = self.t();
        let ctx = ui.ctx().clone();
        self.status_header(ui);
        let muted = self.mute.is_muted();
        let (out_on, mic_on, statuses) = match &self.phase {
            Phase::Recording(r) => (r.shared.out_active(), r.shared.mic_active() && !muted, Some(r.statuses())),
            _ => (false, false, None),
        };
        ui.horizontal(|ui| {
            ui::indicator(ui, out_on, GREEN, t.ind_output);
            ui.add_space(18.0);
            ui::indicator(ui, mic_on, ORANGE, t.ind_mic);
            let (r, resp) = ui.allocate_exact_size(egui::Vec2::splat(20.0), egui::Sense::hover());
            let icon = if muted { Icon::MicOff } else { Icon::Mic };
            ui::paint_icon(ui.painter(), icon, r, ui.visuals().text_color());
            if muted {
                resp.on_hover_text(t.mic_muted);
            }
        });
        ui.add_space(2.0);
        let recording = matches!(self.phase, Phase::Recording(_));
        let idle = matches!(self.phase, Phase::Idle);
        let paused = matches!(&self.phase, Phase::Recording(r) if r.is_paused());
        let w = (ui.available_width() - 16.0) / 3.0;
        ui.horizontal(|ui| {
            if ui::icon_button(ui, Icon::Rec, t.btn_rec, idle && self.dialog.is_none(), false, w).clicked() {
                self.start(&ctx);
            }
            let (pi, pt) = if paused { (Icon::Play, t.btn_resume) } else { (Icon::Pause, t.btn_pause) };
            if ui::icon_button(ui, pi, pt, recording, paused, w).clicked() {
                self.toggle_pause();
            }
            if ui::icon_button(ui, Icon::Stop, t.btn_stop, recording, false, w).clicked() {
                self.stop(&ctx);
            }
        });
        let w2 = (ui.available_width() - 8.0) / 2.0;
        ui.horizontal(|ui| {
            let (mi, mt) = if muted { (Icon::MicOff, t.btn_unmute) } else { (Icon::Mic, t.btn_mute) };
            if ui::icon_button(ui, mi, mt, true, muted, w2).clicked() {
                self.mute.toggle();
            }
            if ui::icon_button(ui, Icon::Bookmark, t.btn_mark, recording, false, w2).clicked() {
                self.bookmark();
            }
        });
        if let Some((mic, out)) = statuses {
            if mic == CaptureStatus::DeviceMissing {
                ui.colored_label(ORANGE, t.mic_missing);
            }
            if out == CaptureStatus::DeviceMissing {
                ui.colored_label(ORANGE, t.out_missing);
            }
            for (s, what) in [(mic, t.ind_mic), (out, t.ind_output)] {
                if s == CaptureStatus::Failed {
                    ui.colored_label(RED, fill(t.capture_failed, &[("what", what)]));
                }
            }
        }
        self.notices_ui(ui);
    }

    fn notices_ui(&mut self, ui: &mut egui::Ui) {
        let mut remove = None;
        for (i, n) in self.notices.iter().enumerate() {
            ui.horizontal_wrapped(|ui| {
                let col = match n.level {
                    Level::Info => ui.visuals().weak_text_color(),
                    Level::Warn => ORANGE,
                };
                if ui.small_button("✕").clicked() {
                    remove = Some(i);
                }
                ui.label(RichText::new(&n.text).color(col).size(12.0));
            });
        }
        if let Some(i) = remove {
            self.notices.remove(i);
        }
    }

    fn save_view(&mut self, ui: &mut egui::Ui) {
        let t = self.t();
        let Phase::Save(save) = &mut self.phase else { return };
        ui.label(RichText::new(fill(t.finished, &[("dur", &format_hms(save.finished.frames))])).strong().size(16.0));
        ui.checkbox(&mut save.both, t.both_formats);
        if !save.labels.is_empty() {
            ui.label(RichText::new(t.bookmarks).strong());
            egui::ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                for (b, l) in save.finished.meta.bookmarks.iter().zip(save.labels.iter_mut()) {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format_hms(b.frame)).monospace());
                        ui.add(egui::TextEdit::singleline(l).desired_width(ui.available_width()));
                    });
                }
            });
        }
        ui.add_space(6.0);
        let w = (ui.available_width() - 8.0) / 2.0;
        let mut do_save = false;
        let mut do_discard = false;
        ui.horizontal(|ui| {
            do_save = ui.add_sized([w, 32.0], egui::Button::new(RichText::new(t.save).strong())).clicked();
            do_discard = ui.add_sized([w, 32.0], egui::Button::new(t.discard)).clicked();
        });
        if do_save && self.dialog.is_none() {
            self.ask_save_path();
        }
        if do_discard {
            self.dialog = Some(Dialog::ConfirmDiscard);
        }
        self.notices_ui(ui);
    }

    fn exporting_view(&mut self, ui: &mut egui::Ui) {
        let t = self.t();
        let Phase::Exporting(job) = &self.phase else { return };
        let p = job.progress.load(Ordering::Relaxed) as f32 / 1000.0;
        ui.add_space(30.0);
        ui.label(RichText::new(t.saving).strong().size(16.0));
        ui.add(egui::ProgressBar::new(p).show_percentage());
    }

    fn source_label(&self, s: &SourceSetting, list: &[DeviceInfo]) -> String {
        let t = self.t();
        match s.mode {
            SourceMode::Communications => t.opt_comm.to_string(),
            SourceMode::System => t.opt_system.to_string(),
            SourceMode::Device => match list.iter().find(|d| d.id == s.id) {
                Some(d) => d.name.clone(),
                None => format!("{} {}", s.name, t.opt_missing),
            },
            SourceMode::App => format!("{} {}", t.opt_app_prefix, s.name),
        }
    }

    fn source_combo(&mut self, ui: &mut egui::Ui, id: &str, output: bool) -> bool {
        let t = self.t();
        let current = if output { self.settings.output.clone() } else { self.settings.input.clone() };
        let list = if output { self.outputs.clone() } else { self.inputs.clone() };
        let mut options: Vec<(SourceSetting, String)> = vec![
            (SourceSetting { mode: SourceMode::Communications, ..Default::default() }, t.opt_comm.into()),
            (SourceSetting { mode: SourceMode::System, ..Default::default() }, t.opt_system.into()),
        ];
        for d in &list {
            options.push((
                SourceSetting { mode: SourceMode::Device, id: d.id.clone(), name: d.name.clone() },
                d.name.clone(),
            ));
        }
        if output {
            let mut apps: Vec<String> = self.apps.iter().map(|a| a.exe.clone()).collect();
            if current.mode == SourceMode::App && !apps.iter().any(|a| a.eq_ignore_ascii_case(&current.name)) {
                apps.push(current.name.clone());
            }
            for a in apps {
                options.push((
                    SourceSetting { mode: SourceMode::App, id: String::new(), name: a.clone() },
                    format!("{} {a}", t.opt_app_prefix),
                ));
            }
        }
        let same = |a: &SourceSetting, b: &SourceSetting| {
            a.mode == b.mode
                && match a.mode {
                    SourceMode::Device => a.id == b.id,
                    SourceMode::App => a.name.eq_ignore_ascii_case(&b.name),
                    _ => true,
                }
        };
        let mut chosen = None;
        egui::ComboBox::from_id_salt(id)
            .width(ui.available_width())
            .selected_text(self.source_label(&current, &list))
            .show_ui(ui, |ui| {
                for (opt, label) in &options {
                    if ui.selectable_label(same(opt, &current), label).clicked() {
                        chosen = Some(opt.clone());
                    }
                }
                if output && self.apps.is_empty() {
                    ui.label(RichText::new(t.app_list_empty).weak().italics());
                }
            });
        match chosen {
            Some(c) if !same(&c, &current) => {
                if output {
                    self.settings.output = c;
                } else {
                    self.settings.input = c.clone();
                    self.mute.set_input(c);
                }
                true
            }
            _ => false,
        }
    }

    fn settings_view(&mut self, ui: &mut egui::Ui) {
        let t = self.t();
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new(t.settings).strong().size(16.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(t.close).clicked() {
                    self.set_view(View::Main);
                }
            });
        });
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let recording = !matches!(self.phase, Phase::Idle | Phase::Save(_));
            ui.add_enabled_ui(!recording, |ui| {
                ui.label(RichText::new(t.input_device).strong());
                changed |= self.source_combo(ui, "input", false);
                ui.label(RichText::new(t.output_source).strong());
                changed |= self.source_combo(ui, "output", true);
                if ui.small_button(t.refresh).clicked() {
                    self.refresh_lists();
                }
            });
            if recording {
                ui.label(RichText::new(t.sources_locked).weak().italics());
            }
            ui.separator();
            ui.label(RichText::new(t.format).strong());
            changed |= ui.radio_value(&mut self.settings.format, Format::Stereo, t.format_stereo).changed();
            changed |= ui.radio_value(&mut self.settings.format, Format::Mono, t.format_mono).changed();
            changed |= ui.checkbox(&mut self.settings.both_formats, t.both_formats).changed();
            ui.horizontal(|ui| {
                ui.label(t.bitrate);
                let label = |k: u32| format!("{k} kbps · ~{} MB/h", (k as f32 * 0.45).round());
                egui::ComboBox::from_id_salt("bitrate").selected_text(label(self.settings.bitrate_kbps)).show_ui(
                    ui,
                    |ui| {
                        for k in BITRATES {
                            changed |= ui.selectable_value(&mut self.settings.bitrate_kbps, k, label(k)).changed();
                        }
                    },
                );
            });
            changed |= ui.checkbox(&mut self.settings.normalize, t.normalize).changed();
            changed |= ui.checkbox(&mut self.settings.autosave, t.autosave).changed();
            ui.separator();
            ui.label(RichText::new(t.hotkeys).strong());
            ui.label(RichText::new(t.hk_hint).weak().size(12.0));
            egui::Grid::new("hotkeys").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                for (a, name) in [
                    (Action::Start, t.hk_start),
                    (Action::Pause, t.hk_pause),
                    (Action::Stop, t.hk_stop),
                    (Action::Mute, t.hk_mute),
                    (Action::Bookmark, t.hk_bookmark),
                ] {
                    ui.label(name);
                    let text = if self.capturing_hotkey == Some(a) {
                        t.hk_press.to_string()
                    } else {
                        self.settings.hotkeys.get(a).to_string()
                    };
                    if ui.button(text).clicked() && self.capturing_hotkey.is_none() {
                        self.capturing_hotkey = Some(a);
                        self.hotkeys.unregister_all();
                    }
                    ui.end_row();
                }
            });
            if let Some(a) = self.capturing_hotkey {
                let ev = ui.input(|i| {
                    i.events.iter().find_map(|e| match e {
                        egui::Event::Key { key, pressed: true, modifiers, .. } => Some((*key, *modifiers)),
                        _ => None,
                    })
                });
                if let Some((key, mods)) = ev {
                    if key == egui::Key::Escape {
                        self.capturing_hotkey = None;
                        self.register_hotkeys();
                    } else if let Some(combo) = combo_from(key, mods) {
                        self.settings.hotkeys.set(a, combo);
                        self.capturing_hotkey = None;
                        self.notices.clear();
                        self.register_hotkeys();
                        changed = true;
                    }
                }
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(RichText::new(t.language).strong());
                let name = |l: Lang| match l {
                    Lang::Auto => t.lang_auto,
                    Lang::It => "Italiano",
                    Lang::En => "English",
                };
                egui::ComboBox::from_id_salt("lang").selected_text(name(self.settings.language)).show_ui(ui, |ui| {
                    for l in [Lang::Auto, Lang::It, Lang::En] {
                        changed |= ui.selectable_value(&mut self.settings.language, l, name(l)).changed();
                    }
                });
            });
            ui.add_space(4.0);
            ui.label(RichText::new(format!("{APP_NAME} {}", simpleaudiorec_core::APP_VERSION)).weak().size(11.0));
        });
        if changed {
            self.save_settings();
        }
    }

    fn dialog_ui(&mut self, ctx: &egui::Context) {
        let t = self.t();
        let Some(d) = self.dialog.take() else { return };
        let mut keep = true;
        let mut next: Option<Dialog> = None;
        let modal = egui::Modal::new(egui::Id::unique("simpleaudiorec-dialog")).show(ctx, |ui| {
            ui.set_max_width(360.0);
            match &d {
                Dialog::CloseWhileRecording => {
                    ui.label(t.close_recording);
                    ui.horizontal(|ui| {
                        if ui.button(RichText::new(t.stop_and_save).strong()).clicked() {
                            keep = false;
                            self.stop(ctx);
                        }
                        if ui.button(t.cancel).clicked() {
                            keep = false;
                        }
                    });
                }
                Dialog::ConfirmDiscard => {
                    ui.label(t.confirm_discard);
                    ui.horizontal(|ui| {
                        if ui.button(t.yes).clicked() {
                            keep = false;
                            self.discard_current();
                        }
                        if ui.button(t.no).clicked() {
                            keep = false;
                        }
                    });
                }
                Dialog::Recovery(u) => {
                    let date = u.meta.start_time().map(|s| s.format("%d/%m/%Y %H:%M").to_string()).unwrap_or_default();
                    ui.label(fill(t.recovery, &[("date", &date), ("dur", &format_hms(u.frames))]));
                    ui.horizontal(|ui| {
                        if ui.button(RichText::new(t.recover).strong()).clicked() {
                            keep = false;
                            let finished = Finished { session: u.dir.clone(), meta: u.meta.clone(), frames: u.frames };
                            self.phase = Phase::Save(SaveState {
                                labels: finished.meta.bookmarks.iter().map(|b| b.label.clone()).collect(),
                                both: self.settings.both_formats,
                                finished,
                            });
                        }
                        if ui.button(t.discard).clicked() {
                            keep = false;
                            next = Some(Dialog::ConfirmDiscardRecovered(u.clone()));
                        }
                    });
                }
                Dialog::ConfirmDiscardRecovered(u) => {
                    ui.label(t.confirm_discard);
                    ui.horizontal(|ui| {
                        if ui.button(t.yes).clicked() {
                            keep = false;
                            let _ = u.dir.discard();
                        }
                        if ui.button(t.no).clicked() {
                            keep = false;
                            next = Some(Dialog::Recovery(u.clone()));
                        }
                    });
                }
            }
        });
        let _ = modal;
        if keep {
            self.dialog = Some(d);
        } else if next.is_some() {
            self.dialog = next;
        } else {
            self.next_recovery();
        }
    }

    fn update_window(&mut self, ctx: &egui::Context) {
        let t = self.t();
        let size = match (&self.view, &self.phase) {
            (View::Settings, _) => SETTINGS_SIZE,
            (_, Phase::Save(_)) => SAVE_SIZE,
            _ => MAIN_SIZE,
        };
        if size != self.last_size {
            self.last_size = size;
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(size.into()));
        }
        let (title, overlay) = match &self.phase {
            Phase::Recording(r) if r.is_paused() => {
                (format!("{} {} — {APP_NAME}", t.paused, format_hms(r.shared.frames())), Some([0x9A, 0x9A, 0x9A]))
            }
            Phase::Recording(r) => (format!("● REC {} — {APP_NAME}", format_hms(r.shared.frames())), Some([0xE5, 0x48, 0x4D])),
            _ => (APP_NAME.to_string(), None),
        };
        if title != self.last_title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
        if let Some(tb) = &mut self.taskbar {
            tb.set(overlay, t.rec);
        }
        if let Some(r) = ctx.input(|i| i.viewport().outer_rect) {
            self.window_pos = Some([r.min.x, r.min.y]);
        }
    }

    fn check_disk(&mut self) {
        if !matches!(self.phase, Phase::Recording(_)) || self.last_disk_check.elapsed() < Duration::from_secs(10) {
            return;
        }
        self.last_disk_check = Instant::now();
        let dir = self.paths.recovery_dir();
        if let Some(free) = platform::free_disk_bytes(&dir) {
            if free < DISK_LOW_BYTES && !self.disk_warned {
                self.disk_warned = true;
                let s = format!("{:.1} GB", free as f64 / 1e9);
                let msg = fill(self.t().disk_low, &[("free", &s)]);
                self.notify(msg, Level::Warn);
            } else if free >= DISK_LOW_BYTES {
                self.disk_warned = false;
            }
        }
    }
}

fn default_dir() -> PathBuf {
    rfd_documents().unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
}

fn rfd_documents() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    let docs = PathBuf::from(home).join("Documents");
    docs.is_dir().then_some(docs)
}

impl eframe::App for SimpleAudioRecApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        for a in self.hotkeys.poll() {
            self.handle_action(a, ctx);
        }
        let n = self.mute.restored.swap(0, Ordering::Relaxed);
        if n > 0 {
            let msg = self.t().mute_restored.to_string();
            self.notify(msg, Level::Warn);
        }
        if let Phase::Recording(r) = &self.phase {
            r.set_mic_muted(self.mute.is_muted());
        }
        self.poll_export();
        self.check_disk();
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            match &self.phase {
                Phase::Recording(_) => {
                    ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                    ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
                    self.dialog = Some(Dialog::CloseWhileRecording);
                }
                Phase::Exporting(_) => ctx.send_viewport_cmd(ViewportCommand::CancelClose),
                _ => self.allow_close = true,
            }
        }
        self.update_window(ctx);
        let busy = matches!(self.phase, Phase::Recording(_) | Phase::Exporting(_));
        ctx.request_repaint_after(Duration::from_millis(if busy { 100 } else { 500 }));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        egui::Frame::central_panel(ui.style()).inner_margin(14.0).show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            match (&self.view, &self.phase) {
                (View::Settings, _) => self.settings_view(ui),
                (_, Phase::Save(_)) => self.save_view(ui),
                (_, Phase::Exporting(_)) => self.exporting_view(ui),
                _ => self.main_view(ui),
            }
        });
        self.dialog_ui(&ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Phase::Recording(r) = std::mem::replace(&mut self.phase, Phase::Idle) {
            // Chiusura forzata: la registrazione resta su disco per il recupero.
            let _ = r.stop();
        }
        self.mute.release();
        self.hotkeys.unregister_all();
        if let Some(p) = self.window_pos {
            self.settings.window_pos = Some(p);
        }
        self.save_settings();
    }
}
