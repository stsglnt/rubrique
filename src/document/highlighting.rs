//! Color Markdown, YAML and Helm template syntax in the editor with the
//! current theme, and lay plain text out with the same line height.

use eframe::egui::cache::{ComputerMut, FrameCache};
use eframe::egui::text::LayoutJob;
use eframe::egui::{Color32, Context, FontId, Stroke, TextFormat, TextStyle, Ui};

use std::path::Path;

use crate::markdown::highlight::{Kind, Span};
use crate::theme::style::EDITOR_LINE_HEIGHT;

/// Which syntax the editor colors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Syntax {
    Markdown,
    /// YAML, with Helm (Go template) actions.
    Yaml,
}

impl Syntax {
    /// The syntax of a file, from its name; `None` for plain text.
    pub fn of(path: &Path) -> Option<Syntax> {
        if crate::explorer::tree::is_markdown_path(path) {
            Some(Syntax::Markdown)
        } else if crate::helm::is_yaml_path(path) {
            Some(Syntax::Yaml)
        } else {
            None
        }
    }

    fn spans(self, text: &str) -> Vec<Span> {
        match self {
            Syntax::Markdown => crate::markdown::highlight::highlight(text),
            Syntax::Yaml => crate::helm::highlight::highlight(text),
        }
    }
}

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
    /// columns stay lined up, and the editor's line height.
    fn format(&self, kind: Kind) -> TextFormat {
        let plain = TextFormat {
            font_id: self.font.clone(),
            color: self.text,
            line_height: Some(editor_line_height(&self.font)),
            ..TextFormat::default()
        };
        match kind {
            Kind::Plain => plain,
            Kind::Marker | Kind::Url => TextFormat { color: self.weak, ..plain },
            Kind::Heading | Kind::LinkText | Kind::Key => TextFormat { color: self.accent, ..plain },
            Kind::Strong => TextFormat { color: self.strong, ..plain },
            Kind::Emphasis => TextFormat { italics: true, ..plain },
            Kind::Strikethrough => TextFormat { color: self.weak, strikethrough: Stroke::new(1.0, self.weak), ..plain },
            Kind::Code => TextFormat { background: self.code_background, ..plain },
            Kind::Template => TextFormat { color: self.strong, background: self.code_background, ..plain },
            Kind::Quote | Kind::Comment => TextFormat { color: self.weak, italics: true, ..plain },
        }
    }
}

/// The height of one line of `font` in the source editor and the block
/// editor: taller than the font's own rows, so lines of text breathe.
pub fn editor_line_height(font: &FontId) -> f32 {
    (font.size * EDITOR_LINE_HEIGHT).round()
}

/// The layout of `text`, colored for `syntax` (plain when `None`), reused
/// while the text and theme stay the same.
pub fn highlighted(ctx: &Context, text: &str, style: &HighlightStyle, syntax: Option<Syntax>) -> LayoutJob {
    ctx.memory_mut(|memory| memory.caches.cache::<HighlightCache>().get((text, style, syntax)).clone())
}

type HighlightCache = FrameCache<LayoutJob, Highlighter>;

/// Builds layouts for the cache.
#[derive(Default)]
struct Highlighter;

impl ComputerMut<(&str, &HighlightStyle, Option<Syntax>), LayoutJob> for Highlighter {
    fn compute(&mut self, (text, style, syntax): (&str, &HighlightStyle, Option<Syntax>)) -> LayoutJob {
        layout_job(text, style, syntax)
    }
}

/// One text section per highlighted span; plain text is one section.
fn layout_job(text: &str, style: &HighlightStyle, syntax: Option<Syntax>) -> LayoutJob {
    let mut job = LayoutJob::default();
    match syntax {
        Some(syntax) => {
            for span in syntax.spans(text) {
                job.append(&text[span.range], 0.0, style.format(span.kind));
            }
        }
        None => job.append(text, 0.0, style.format(Kind::Plain)),
    }
    job
}
