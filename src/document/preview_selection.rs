//! Remember the text selected in the preview, for "Add note".
//!
//! egui reports a finished label selection only at the end of a pass, as an
//! output command; this plugin, registered after egui's own, catches it and
//! keeps it (with where the pointer was) for the next frame.

use eframe::egui::{self, Context, Id, OutputCommand, Pos2, Ui};

/// The selected preview text and the pointer position when it was made.
#[derive(Clone, Debug)]
pub struct PreviewSelection {
    pub text: String,
    pub pointer: Pos2,
}

/// Install the plugin once, when the app starts.
pub fn install(ctx: &Context) {
    ctx.add_plugin(SelectionTracker::default());
}

/// Pointer movement (points) between press and release that counts as a
/// drag; a plain click on a label also reports its text, which is not a
/// selection the user made.
const MIN_DRAG: f32 = 4.0;

/// The current preview selection, if any.
pub fn current(ctx: &Context) -> Option<PreviewSelection> {
    ctx.data(|data| data.get_temp::<PreviewSelection>(selection_id()))
}

/// Forget the selection (after a note was added).
pub fn clear(ctx: &Context) {
    ctx.data_mut(|data| data.remove::<PreviewSelection>(selection_id()));
    ctx.plugin::<egui::text_selection::LabelSelectionState>().lock().clear_selection();
}

fn selection_id() -> Id {
    Id::unique("rubrique-preview-selection")
}

#[derive(Default)]
struct SelectionTracker {
    /// Where the primary button went down.
    press_origin: Option<Pos2>,
}

impl egui::Plugin for SelectionTracker {
    fn debug_name(&self) -> &'static str {
        "rubrique selection tracker"
    }

    fn on_begin_pass(&mut self, ui: &mut Ui) {
        if let Some(origin) = ui.ctx().input(|input| input.pointer.press_origin()) {
            self.press_origin = Some(origin);
        }
    }

    fn on_end_pass(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let settled = ctx.output(|output| {
            output.commands.iter().rev().find_map(|command| match command {
                OutputCommand::TextSelectionSettled(text) => Some(text.clone()),
                _ => None,
            })
        });
        let has_selection = ctx.plugin::<egui::text_selection::LabelSelectionState>().lock().has_selection();
        if !has_selection {
            ctx.data_mut(|data| data.remove::<PreviewSelection>(selection_id()));
            return;
        }
        if let Some(text) = settled.filter(|text| !text.trim().is_empty()) {
            let pointer = ctx.input(|input| input.pointer.latest_pos()).unwrap_or_default();
            let dragged = self.press_origin.is_some_and(|origin| origin.distance(pointer) >= MIN_DRAG);
            if !dragged {
                return;
            }
            ctx.data_mut(|data| data.insert_temp(selection_id(), PreviewSelection { text, pointer }));
        }
    }
}
