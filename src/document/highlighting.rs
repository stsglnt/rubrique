//! Color Markdown syntax in the editor with the current theme.

use eframe::egui::cache::{ComputerMut, FrameCache};
use eframe::egui::text::LayoutJob;
use eframe::egui::{Color32, Context, FontId, Stroke, TextFormat, TextStyle, Ui};

use crate::markdown::highlight::{Kind, highlight};

/// Theme colors and the editor font, read from the current style.
#[derive(Clone, Debug, PartialEq, Hash)]
pub struct HighlightStyle {
    font: FontId,
    text: Color32,
    strong: Color32,
    weak: Color32,
    accent: Color32,
    code_background: Color32,
}

impl HighlightStyle {
    /// The style for `ui`'s theme.
    pub fn from_ui(ui: &Ui) -> Self {
        let visuals = ui.visuals();
        HighlightStyle {
            font: TextStyle::Monospace.resolve(ui.style()),
            text: visuals.text_color(),
            strong: visuals.strong_text_color(),
            weak: visuals.weak_text_color(),
            accent: visuals.hyperlink_color,
            code_background: visuals.code_bg_color,
        }
    }

    /// How to draw one kind of text. Every kind keeps the monospace font, so
    /// columns stay lined up.
    fn format(&self, kind: Kind) -> TextFormat {
        let plain = TextFormat { font_id: self.font.clone(), color: self.text, ..TextFormat::default() };
        match kind {
            Kind::Plain => plain,
            Kind::Marker | Kind::Url => TextFormat { color: self.weak, ..plain },
            Kind::Heading | Kind::LinkText => TextFormat { color: self.accent, ..plain },
            Kind::Strong => TextFormat { color: self.strong, ..plain },
            Kind::Emphasis => TextFormat { italics: true, ..plain },
            Kind::Strikethrough => TextFormat { color: self.weak, strikethrough: Stroke::new(1.0, self.weak), ..plain },
            Kind::Code => TextFormat { background: self.code_background, ..plain },
            Kind::Quote | Kind::Comment => TextFormat { color: self.weak, italics: true, ..plain },
        }
    }
}

/// The colored layout of `text`, reused while the text and theme stay the same.
pub fn highlighted(ctx: &Context, text: &str, style: &HighlightStyle) -> LayoutJob {
    ctx.memory_mut(|memory| memory.caches.cache::<HighlightCache>().get((text, style)).clone())
}

type HighlightCache = FrameCache<LayoutJob, Highlighter>;

/// Builds layouts for the cache.
#[derive(Default)]
struct Highlighter;

impl ComputerMut<(&str, &HighlightStyle), LayoutJob> for Highlighter {
    fn compute(&mut self, (text, style): (&str, &HighlightStyle)) -> LayoutJob {
        layout_job(text, style)
    }
}

/// One text section per highlighted span.
fn layout_job(text: &str, style: &HighlightStyle) -> LayoutJob {
    let mut job = LayoutJob::default();
    for span in highlight(text) {
        job.append(&text[span.range], 0.0, style.format(span.kind));
    }
    job
}
