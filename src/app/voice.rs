//! Voice memos in the app: record, add audio files (dialog or drag and
//! drop), play memos in the preview, and transcribe them into the note.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use eframe::egui::{self, Context, Ui};

use super::{OpenFile, RubriqueApp, char_to_byte};
use crate::ai::format_elapsed;
use crate::ai::run::{AiRun, RunEnd};
use crate::document::merge::MergeSession;
use crate::document::view::{editor_cursor, set_editor_cursor};
use crate::explorer::paths::name_text;
use crate::explorer::tree::FsNode;
use crate::markdown::blocks::{Block, SourceBlock};
use crate::voice::audio::{Player, Recorder};
use crate::voice::view::{MemoAction, MemoState, MemoStates, RecordingChoice, RecordingInfo, recording_bar};
use crate::voice::{
    NoteKind, add_transcript, clean_transcript, free_path, is_audio_path, is_wav, memo_file_name, memo_label,
    memo_link, reference, relative_url, resolve_url, wav,
};

/// Seconds of silence before the bar suggests checking the microphone.
const SILENT_HINT_SECONDS: u64 = 3;
/// Recordings shorter than this are dropped as accidental.
const SHORTEST_MEMO_SECONDS: f64 = 0.5;
/// How often the window redraws while recording (for the time and meter).
const RECORDING_REPAINT: Duration = Duration::from_millis(50);
/// How often the window redraws while a memo plays.
const PLAYING_REPAINT: Duration = Duration::from_millis(100);
/// How fast the level meter falls back.
const METER_FALL: f32 = 0.85;
/// Bytes read to find a WAV file's length.
const HEADER_READ: usize = 4096;

/// Recording, playback and transcription state.
#[derive(Debug, Default)]
pub(super) struct VoiceState {
    recording: Option<Recording>,
    /// The memo file loaded for playback.
    player: Option<(PathBuf, Player)>,
    transcribing: Option<Transcribing>,
    /// Length of each WAV memo seen in the preview (`None`: not a readable WAV).
    durations: HashMap<PathBuf, Option<f64>>,
    /// The top-level preview block under the pointer, for drops.
    pub(super) preview_hover: Option<usize>,
    /// Where the transcription program is, checked when Settings opens.
    pub(super) program_found: Option<PathBuf>,
}

impl VoiceState {
    /// Stop playback (when another file opens).
    pub(super) fn stop_playback(&mut self) {
        self.player = None;
    }

    /// What the preview should show for each memo in `blocks`.
    pub(super) fn memo_states(&mut self, folder: &Path, blocks: &[SourceBlock]) -> MemoStates {
        let mut urls = Vec::new();
        for block in blocks {
            collect_memo_urls(&block.block, &mut urls);
        }
        let transcribing = self.transcribing.as_ref();
        let mut states = MemoStates::default();
        for url in urls {
            let Some(path) = resolve_url(folder, &url) else {
                let state = MemoState { busy: transcribing.is_some(), ..MemoState::default() };
                states.by_url.insert(url, state);
                continue;
            };
            let missing = !path.is_file();
            let duration = if missing || !is_wav(&path) {
                None
            } else {
                *self.durations.entry(path.clone()).or_insert_with(|| wav_duration(&path))
            };
            let player = self.player.as_ref().filter(|(playing, _)| *playing == path).map(|(_, player)| player);
            let running = transcribing.filter(|job| job.url == url && job.run.is_running());
            let state = MemoState {
                missing,
                playable: duration.is_some(),
                duration,
                position: player.map_or(0.0, Player::position_seconds),
                playing: player.is_some_and(Player::is_playing),
                transcribing: running.map(|job| job.run.elapsed_seconds()),
                busy: transcribing.is_some_and(|job| job.run.is_running()),
            };
            states.by_url.insert(url, state);
        }
        states
    }
}

#[derive(Debug)]
struct Recording {
    recorder: Recorder,
    /// The note the memo is for.
    note: PathBuf,
    label: String,
    level: f32,
}

#[derive(Debug)]
struct Transcribing {
    run: AiRun,
    note: PathBuf,
    /// The memo's link target as written in the note.
    url: String,
    program: String,
}

impl RubriqueApp {
    /// The open note and its kind, when it can hold memos.
    fn memo_note(&self) -> Option<(PathBuf, NoteKind)> {
        let OpenFile::Text { document, .. } = &self.open_file else {
            return None;
        };
        NoteKind::of(&document.path).map(|kind| (document.path.clone(), kind))
    }

    /// Start recording, or stop and add the memo when already recording.
    pub(super) fn toggle_recording(&mut self, ctx: &Context) {
        if self.voice.recording.is_some() {
            self.stop_recording(ctx);
        } else {
            self.start_recording();
        }
    }

    fn start_recording(&mut self) {
        let Some((note, _)) = self.memo_note() else {
            self.show_status("Open a Markdown or HTML file to record a voice memo".to_string());
            return;
        };
        let folder = self.settings.voice.memos_folder(note.parent().unwrap_or(Path::new("/")));
        if let Err(error) = std::fs::create_dir_all(&folder) {
            self.show_status(format!("Cannot create {}: {error}", folder.display()));
            return;
        }
        let now = crate::date::now_local();
        let path = free_path(&folder, &memo_file_name(&now), Path::exists);
        self.voice.player = None;
        match Recorder::start(&path) {
            Ok(recorder) => {
                self.voice.recording = Some(Recording { recorder, note, label: memo_label(&now), level: 0.0 });
            }
            Err(error) => self.show_status(error),
        }
    }

    /// Stop recording and add the memo to its note.
    fn stop_recording(&mut self, ctx: &Context) {
        let Some(Recording { recorder, note, label, .. }) = self.voice.recording.take() else {
            return;
        };
        let path = recorder.path().to_path_buf();
        match recorder.finish() {
            Ok(seconds) if seconds < SHORTEST_MEMO_SECONDS => {
                let _ = std::fs::remove_file(&path);
                self.show_status("The recording was too short, so it was not kept".to_string());
                return;
            }
            Ok(_) => {}
            Err(error) => {
                self.show_status(format!("Cannot finish the memo: {error}"));
                return;
            }
        }
        self.refresh_explorer();
        if self.open_file.path() != Some(note.as_path()) {
            self.show_status(format!("Saved {}. Open {} to add it.", path.display(), name_text(&note)));
            return;
        }
        let Some(url) = self.insert_memo(ctx, &path, &label) else {
            return;
        };
        if self.settings.voice.auto_transcribe && self.settings.voice.find_program().is_some() {
            self.start_transcription(&url);
        } else {
            self.show_status(format!("Added {} (save to keep the link)", name_text(&path)));
        }
    }

    fn cancel_recording(&mut self) {
        if let Some(recording) = self.voice.recording.take() {
            recording.recorder.cancel();
            self.show_status("Recording deleted".to_string());
        }
    }

    /// Link `audio` into the open note where the cursor (or the drop) is.
    /// Returns the link target as written.
    fn insert_memo(&mut self, ctx: &Context, audio: &Path, label: &str) -> Option<String> {
        self.finish_block_edit();
        let showing_preview = self.showing_preview();
        let hovered = self.voice.preview_hover;
        let OpenFile::Text { document, preview, .. } = &mut self.open_file else {
            return None;
        };
        let kind = NoteKind::of(&document.path)?;
        let folder = document.path.parent().unwrap_or(Path::new("/"));
        let url = relative_url(folder, audio);
        let block_end = |blocks: &Vec<SourceBlock>| hovered.and_then(|index| blocks.get(index)).map(|b| b.range.end);
        let at = if showing_preview {
            preview.as_ref().and_then(block_end)
        } else {
            editor_cursor(ctx).map(|cursor| char_to_byte(&document.text, cursor))
        };
        let at = at.unwrap_or(document.text.len());
        let (text, cursor) = crate::voice::insert_block(&document.text, at, &reference(kind, &url, label), kind);
        document.text = text;
        document.on_text_changed();
        *preview = None;
        set_editor_cursor(ctx, cursor);
        Some(url)
    }

    /// Add an audio file to the open note: copied into the memos folder
    /// when it comes from elsewhere (unless Settings says to link it in place).
    fn add_audio_file(&mut self, ctx: &Context, audio: &Path) {
        let Some((note, _)) = self.memo_note() else {
            self.show_status("Open a Markdown or HTML file to add audio".to_string());
            return;
        };
        if !is_audio_path(audio) {
            self.show_status(format!("{} is not an audio file", name_text(audio)));
            return;
        }
        let note_folder = note.parent().unwrap_or(Path::new("/")).to_path_buf();
        let target = if audio.starts_with(&note_folder) || !self.settings.voice.copy_dropped {
            audio.to_path_buf()
        } else {
            match self.copy_into_memos(&note_folder, audio) {
                Ok(target) => target,
                Err(error) => {
                    self.show_status(format!("Cannot copy {}: {error}", name_text(audio)));
                    return;
                }
            }
        };
        let label = audio.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
        if self.insert_memo(ctx, &target, &label).is_some() {
            self.refresh_explorer();
            self.show_status(format!("Added {} (save to keep the link)", name_text(&target)));
        }
    }

    /// Copy `audio` into the memos folder, reusing a copy that is already there.
    fn copy_into_memos(&self, note_folder: &Path, audio: &Path) -> std::io::Result<PathBuf> {
        let folder = self.settings.voice.memos_folder(note_folder);
        std::fs::create_dir_all(&folder)?;
        let name = name_text(audio);
        let same = folder.join(&name);
        if same.is_file() && std::fs::read(&same)? == std::fs::read(audio)? {
            return Ok(same);
        }
        let target = free_path(&folder, &name, Path::exists);
        std::fs::copy(audio, &target)?;
        Ok(target)
    }

    /// Pick an audio file in the system dialog and add it.
    pub(super) fn choose_audio_file(&mut self, ctx: &Context) {
        let Some((note, _)) = self.memo_note() else {
            self.show_status("Open a Markdown or HTML file to add audio".to_string());
            return;
        };
        let mut dialog = rfd::FileDialog::new()
            .set_title("Add audio to the note")
            .add_filter("Audio", &["wav", "m4a", "mp3", "ogg", "oga", "opus", "flac", "webm", "aac"]);
        if let Some(folder) = note.parent() {
            dialog = dialog.set_directory(folder);
        }
        if let Some(path) = dialog.pick_file() {
            self.add_audio_file(ctx, &path);
        }
    }

    /// Add audio files dropped on the window.
    pub(super) fn handle_dropped_files(&mut self, ctx: &Context) {
        let dropped: Vec<PathBuf> =
            ctx.input(|input| input.raw.dropped_files.iter().map(|file| file.path().to_path_buf()).collect());
        if dropped.is_empty() {
            return;
        }
        let (audio, other): (Vec<PathBuf>, Vec<PathBuf>) = dropped.into_iter().partition(|path| is_audio_path(path));
        for path in &audio {
            self.add_audio_file(ctx, path);
        }
        if audio.is_empty() && !other.is_empty() {
            self.show_status("Drop audio files (WAV, M4A, MP3, OGG, FLAC…) to add a voice memo".to_string());
        }
    }

    /// While files are dragged over the window: a hint over the page.
    pub(super) fn drop_hint(&self, ctx: &Context, page: egui::Rect) {
        if !ctx.input(|input| !input.raw.hovered_files.is_empty()) {
            return;
        }
        let text = if self.memo_note().is_some() {
            "Drop audio to add a voice memo here"
        } else {
            "Open a Markdown or HTML file to add audio"
        };
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("drop-hint")));
        let visuals = ctx.global_style().visuals.clone();
        painter.rect_filled(page.shrink(6.0), 10, visuals.hyperlink_color.gamma_multiply(0.08));
        painter.rect_stroke(
            page.shrink(6.0),
            10,
            egui::Stroke::new(1.5, visuals.hyperlink_color.gamma_multiply(0.6)),
            egui::StrokeKind::Inside,
        );
        let at = egui::pos2(page.center().x, page.top() + 40.0);
        let font = egui::FontId::proportional(15.0);
        painter.text(at, egui::Align2::CENTER_CENTER, text, font, visuals.strong_text_color());
    }

    fn refresh_explorer(&mut self) {
        self.roots.iter_mut().for_each(FsNode::refresh);
    }

    /// Each frame: write recorded audio, follow playback and transcription.
    pub(super) fn poll_voice(&mut self, ctx: &Context) {
        if let Some(recording) = &mut self.voice.recording {
            if let Err(error) = recording.recorder.write_pending() {
                self.voice.recording = None;
                self.show_status(format!("Recording stopped: {error}"));
            } else {
                ctx.request_repaint_after(RECORDING_REPAINT);
            }
        }
        if self.voice.player.as_ref().is_some_and(|(_, player)| player.is_playing()) {
            ctx.request_repaint_after(PLAYING_REPAINT);
        }
        let ended = self.voice.transcribing.as_mut().and_then(|job| job.run.poll().cloned());
        if let Some(end) = ended {
            self.finish_transcription(&end);
        } else if self.voice.transcribing.is_some() {
            ctx.request_repaint_after(Duration::from_secs(1));
        }
    }

    /// The bar above the page while recording.
    pub(super) fn recording_bar(&mut self, ui: &mut Ui) {
        let Some(recording) = &mut self.voice.recording else {
            return;
        };
        let level = recording.recorder.take_level();
        recording.level = level.max(recording.level * METER_FALL);
        let seconds = recording.recorder.elapsed_seconds();
        let info = RecordingInfo {
            seconds,
            level: recording.level,
            silent_hint: seconds >= SILENT_HINT_SECONDS && !recording.recorder.heard_sound(),
            error: recording.recorder.error(),
        };
        match recording_bar(ui, &info) {
            Some(RecordingChoice::Stop) => self.stop_recording(ui.ctx()),
            Some(RecordingChoice::Cancel) => self.cancel_recording(),
            None => {}
        }
    }

    /// A button on a memo in the preview.
    pub(super) fn memo_action(&mut self, ctx: &Context, action: MemoAction) {
        let folder = self.open_file.path().and_then(Path::parent).map(Path::to_path_buf).unwrap_or_default();
        match action {
            MemoAction::Play(url) => {
                if let Some(path) = resolve_url(&folder, &url)
                    && let Some(player) = self.player_for(&path)
                {
                    player.play();
                }
            }
            MemoAction::Pause => {
                if let Some((_, player)) = &self.voice.player {
                    player.pause();
                }
            }
            MemoAction::Seek(url, seconds) => {
                if let Some(path) = resolve_url(&folder, &url)
                    && let Some(player) = self.player_for(&path)
                {
                    player.seek(seconds);
                }
            }
            MemoAction::Transcribe(url) => self.start_transcription(&url),
            MemoAction::StopTranscribing => {
                if let Some(job) = &mut self.voice.transcribing {
                    job.run.stop();
                }
            }
            MemoAction::Open(url) => match resolve_url(&folder, &url) {
                Some(path) => {
                    if let Err(error) = crate::media::open_with_system(&path) {
                        self.show_status(error);
                    }
                }
                None => ctx.open_url(egui::OpenUrl::new_tab(url)),
            },
        }
    }

    /// The player for `path`, loading it first when it is not the one loaded.
    fn player_for(&mut self, path: &Path) -> Option<&Player> {
        if self.voice.player.as_ref().is_none_or(|(loaded, _)| loaded != path) {
            self.voice.player = None;
            let loaded = std::fs::read(path)
                .map_err(|error| format!("Cannot read {}: {error}", name_text(path)))
                .and_then(|bytes| wav::decode(&bytes).map_err(|error| format!("Cannot play: {}", error.message())))
                .and_then(Player::new);
            match loaded {
                Ok(player) => self.voice.player = Some((path.to_path_buf(), player)),
                Err(error) => {
                    self.show_status(error);
                    return None;
                }
            }
        }
        self.voice.player.as_ref().map(|(_, player)| player)
    }

    /// Run the transcription command on the memo that `url` links.
    fn start_transcription(&mut self, url: &str) {
        if self.voice.transcribing.as_ref().is_some_and(|job| job.run.is_running()) {
            self.show_status("Wait for the transcription to finish, or press Stop".to_string());
            return;
        }
        let Some((note, _)) = self.memo_note() else {
            return;
        };
        let folder = note.parent().unwrap_or(Path::new("/")).to_path_buf();
        let Some(audio) = resolve_url(&folder, url).filter(|path| path.is_file()) else {
            self.show_status("Only memo files on this computer can be transcribed".to_string());
            return;
        };
        let voice = &self.settings.voice;
        if let Some(problem) = voice.check_command() {
            self.show_status(format!("Transcription command: {} (Settings → Voice memos)", problem.message()));
            return;
        }
        let Some((program, args)) = voice.command_args(&audio) else {
            return;
        };
        if voice.find_program().is_none() {
            self.show_status(format!(
                "{program} was not found. Install whisper.cpp, or set the command in Settings → Voice memos."
            ));
            return;
        }
        if let Some(model) = voice.model_path().filter(|_| voice.model_missing()) {
            self.show_status(format!("Model not found: {}. See Settings → Voice memos.", model.display()));
            return;
        }
        let repaint = self.repaint.clone();
        match AiRun::start(&program, &args, &folder, move || repaint.request_repaint()) {
            Ok(run) => {
                let program = name_text(Path::new(&program));
                self.voice.transcribing = Some(Transcribing { run, note, url: url.to_string(), program });
                if !self.showing_preview() {
                    self.show_status("Transcribing the memo…".to_string());
                }
            }
            Err(error) => self.show_status(format!("Cannot start {program}: {error}")),
        }
    }

    /// The transcription ended: put the text under the memo, through the merge screen.
    fn finish_transcription(&mut self, end: &RunEnd) {
        let Some(job) = self.voice.transcribing.take() else {
            return;
        };
        match end {
            RunEnd::Stopped => {
                self.show_status("Transcription stopped".to_string());
                return;
            }
            RunEnd::Exited { success: false, code } => {
                let errors = job.run.error_text();
                let last = errors.lines().rev().find(|line| !line.trim().is_empty()).unwrap_or("no details");
                let code = code.map(|code| format!(" (exit code {code})")).unwrap_or_default();
                self.show_status(format!("{} failed{code}: {}", job.program, last.trim()));
                return;
            }
            RunEnd::Exited { success: true, .. } => {}
        }
        let transcript = clean_transcript(&job.run.output_text());
        if transcript.is_empty() {
            self.show_status("No speech was found in the memo".to_string());
            return;
        }
        if self.merging.is_some() {
            self.show_status("Finish the merge screen first, then transcribe again".to_string());
            return;
        }
        let OpenFile::Text { document, .. } = &self.open_file else {
            return;
        };
        let Some(kind) = NoteKind::of(&document.path).filter(|_| document.path == job.note) else {
            self.show_status(format!("Open {} to add the transcript, then transcribe again", name_text(&job.note)));
            return;
        };
        let Some(text) = add_transcript(&document.text, kind, &job.url, &transcript) else {
            self.show_status("The memo link is no longer in the file".to_string());
            return;
        };
        let title = format!("Transcript · {} · {}", job.program, format_elapsed(job.run.elapsed_seconds()));
        self.merging = Some(MergeSession::review(&document.text, &text, title));
    }

    /// The Voice memos part of the status bar: Record and Audio….
    pub(super) fn voice_buttons(&mut self, ui: &mut Ui) {
        let recording = self.voice.recording.is_some();
        let tip = if recording {
            "Stop and add the memo (Cmd+Shift+R)"
        } else {
            "Record a voice memo into this note (Cmd+Shift+R)"
        };
        let text = if recording { "Stop" } else { "Record" };
        if crate::icons::icon_text_button(ui, crate::icons::Icon::Mic, text, tip, recording).clicked() {
            self.toggle_recording(ui.ctx());
        }
        let audio = ui.small_button("Audio…").on_hover_text("Add an audio file as a memo (or drop one on the page)");
        if audio.clicked() {
            self.choose_audio_file(ui.ctx());
        }
    }
}

/// Link targets of every memo in a block, nested ones too.
fn collect_memo_urls(block: &Block, urls: &mut Vec<String>) {
    match block {
        Block::Paragraph(content) => urls.extend(memo_link(content).map(|link| link.url)),
        Block::Quote(blocks) => blocks.iter().for_each(|block| collect_memo_urls(block, urls)),
        Block::List(list) => {
            list.items.iter().flat_map(|item| &item.blocks).for_each(|block| collect_memo_urls(block, urls));
        }
        _ => {}
    }
}

/// Length of a WAV file from its header, without reading the samples.
fn wav_duration(path: &Path) -> Option<f64> {
    use std::io::Read as _;
    let file = std::fs::File::open(path).ok()?;
    let length = usize::try_from(file.metadata().ok()?.len()).ok()?;
    let mut start = Vec::with_capacity(HEADER_READ);
    file.take(HEADER_READ as u64).read_to_end(&mut start).ok()?;
    wav::duration_seconds(&start, length)
}
