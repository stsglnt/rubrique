//! Draw parsed Markdown blocks with egui widgets (the read-only preview).

use eframe::egui::{self, RichText, Stroke, Ui};

use super::blocks::{Block, List};
use super::inlines::{Inline, is_openable_url};

/// Heading font sizes for levels 1 to 6.
const HEADING_SIZES: [f32; 6] = [28.0, 24.0, 20.0, 17.0, 15.0, 14.0];
const BODY_SIZE: f32 = 14.0;
/// Vertical gap between blocks.
const BLOCK_GAP: f32 = 8.0;
const QUOTE_BAR_WIDTH: f32 = 3.0;

/// Render a whole document.
pub fn render_blocks(ui: &mut Ui, blocks: &[Block]) {
    for (index, block) in blocks.iter().enumerate() {
        if index > 0 {
            ui.add_space(BLOCK_GAP);
        }
        render_block(ui, block);
    }
}

/// Render one block with the widgets for its kind.
fn render_block(ui: &mut Ui, block: &Block) {
    match block {
        Block::Heading { level, content } => {
            let size = HEADING_SIZES[usize::from(*level).clamp(1, 6) - 1];
            let style = TextStyle { size, strong: true, ..TextStyle::default() };
            render_inlines(ui, content, &style);
        }
        Block::Paragraph(content) => render_inlines(ui, content, &TextStyle::default()),
        Block::Code { info, code } => render_code(ui, info, code),
        Block::Quote(blocks) => render_quote(ui, blocks),
        Block::List(list) => render_list(ui, list),
        Block::ThematicBreak => {
            ui.separator();
        }
    }
}

/// Code block on a tinted background, with its info string as a small label.
fn render_code(ui: &mut Ui, info: &str, code: &str) {
    egui::Frame::new().fill(ui.visuals().code_bg_color).corner_radius(4.0).inner_margin(8.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        if !info.is_empty() {
            ui.label(RichText::new(info).small().weak());
        }
        ui.label(RichText::new(code).monospace());
    });
}

/// Quoted blocks, indented and dimmed, with a bar on the left.
fn render_quote(ui: &mut Ui, blocks: &[Block]) {
    let response = ui.indent("quote", |ui| {
        let weak_color = ui.visuals().weak_text_color();
        ui.visuals_mut().override_text_color = Some(weak_color);
        render_blocks(ui, blocks);
    });
    // Vertical bar on the left of the quoted content.
    let rect = response.response.rect;
    let x = rect.left() - ui.spacing().indent / 2.0;
    let color = ui.visuals().widgets.noninteractive.bg_stroke.color;
    ui.painter()
        .line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())], Stroke::new(QUOTE_BAR_WIDTH, color));
}

/// List items with a bullet, number, or read-only checkbox before each.
fn render_list(ui: &mut Ui, list: &List) {
    for (index, item) in list.items.iter().enumerate() {
        ui.horizontal_top(|ui| {
            match (item.task, list.start) {
                (Some(checked), _) => {
                    let mut checked = checked;
                    ui.add_enabled(false, egui::Checkbox::without_text(&mut checked));
                }
                (None, Some(start)) => {
                    ui.label(format!("{}.", start + index as u64));
                }
                (None, None) => {
                    ui.label("•");
                }
            }
            ui.vertical(|ui| render_blocks(ui, &item.blocks));
        });
    }
}

/// Text formatting accumulated while walking nested inlines.
#[derive(Clone)]
#[expect(clippy::struct_excessive_bools, reason = "independent text styles that combine freely, e.g. bold + italic")]
struct TextStyle {
    /// Font size in points.
    size: f32,
    strong: bool,
    italic: bool,
    strikethrough: bool,
    code: bool,
    /// Target URL when the text is part of a link.
    link: Option<String>,
}

impl Default for TextStyle {
    fn default() -> Self {
        TextStyle { size: BODY_SIZE, strong: false, italic: false, strikethrough: false, code: false, link: None }
    }
}

/// A run of text with one style, or a hard line break.
enum Span {
    Text(String, TextStyle),
    LineBreak,
}

/// Render a paragraph or heading as wrapped, styled text.
fn render_inlines(ui: &mut Ui, inlines: &[Inline], style: &TextStyle) {
    let mut spans = Vec::new();
    collect_spans(inlines, style, &mut spans);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for span in spans {
            match span {
                Span::LineBreak => ui.end_row(),
                Span::Text(text, style) => render_span(ui, text, &style),
            }
        }
    });
}

/// One styled run of text; links become clickable.
fn render_span(ui: &mut Ui, text: String, style: &TextStyle) {
    let mut rich = RichText::new(text).size(style.size);
    if style.strong {
        rich = rich.strong();
    }
    if style.italic {
        rich = rich.italics();
    }
    if style.strikethrough {
        rich = rich.strikethrough();
    }
    if style.code {
        rich = rich.code();
    }
    match &style.link {
        Some(url) if is_openable_url(url) => {
            ui.hyperlink_to(rich, url).on_hover_text(url);
        }
        // Local or unknown targets are shown, not opened (see specs/03).
        Some(url) => {
            ui.label(rich.underline()).on_hover_text(format!("{url} (not opened: only web and mail links open)"));
        }
        None => {
            ui.label(rich);
        }
    }
}

/// Flatten nested inlines into styled spans.
fn collect_spans(inlines: &[Inline], style: &TextStyle, spans: &mut Vec<Span>) {
    for inline in inlines {
        match inline {
            Inline::Text(text) => spans.push(Span::Text(text.clone(), style.clone())),
            Inline::Code(code) => spans.push(Span::Text(code.clone(), TextStyle { code: true, ..style.clone() })),
            Inline::Emphasis(children) => collect_spans(children, &TextStyle { italic: true, ..style.clone() }, spans),
            Inline::Strong(children) => collect_spans(children, &TextStyle { strong: true, ..style.clone() }, spans),
            Inline::Strikethrough(children) => {
                collect_spans(children, &TextStyle { strikethrough: true, ..style.clone() }, spans);
            }
            Inline::Link { content, url } => {
                collect_spans(content, &TextStyle { link: Some(url.clone()), ..style.clone() }, spans);
            }
            Inline::LineBreak => spans.push(Span::LineBreak),
        }
    }
}
