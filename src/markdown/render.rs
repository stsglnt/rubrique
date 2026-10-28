//! Draw parsed Markdown blocks with egui widgets (the read-only preview).

use eframe::egui::{
    self, Align, Align2, Color32, FontId, Layout, Margin, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2,
};

use super::blocks::{Block, List};
use super::inlines::{Inline, is_openable_url};

/// Heading font sizes for levels 1 to 6.
const HEADING_SIZES: [f32; 6] = [30.0, 24.0, 19.0, 16.0, 15.0, 14.0];
/// Space above a heading (levels 1 to 6), larger than below so it groups with its text.
const HEADING_GAPS: [f32; 6] = [28.0, 24.0, 18.0, 14.0, 14.0, 14.0];
/// Line height as a multiple of the font size, for headings.
const HEADING_LINE_FACTOR: f32 = 1.3;
/// Headings up to this level get a thin line underneath.
const RULED_HEADING_LEVEL: u8 = 2;
/// Space between a heading and its line.
const HEADING_RULE_GAP: f32 = 8.0;
const BODY_SIZE: f32 = 15.0;
const BODY_LINE_HEIGHT: f32 = 24.0;
/// Vertical gap between blocks.
const BLOCK_GAP: f32 = 12.0;
/// Vertical gap between list items.
const LIST_ITEM_GAP: f32 = 4.0;
/// Width of the column for bullets, numbers and checkboxes.
const MARKER_WIDTH: f32 = 22.0;
/// Space between a list marker and the item text.
const MARKER_GAP: f32 = 8.0;
const BULLET_RADIUS: f32 = 3.0;
const CHECKBOX_SIZE: f32 = 15.0;
const CHECKBOX_RADIUS: u8 = 4;
const CHECKBOX_STROKE: f32 = 1.5;
/// Check mark points, as fractions of the checkbox size.
const CHECK_MARK: [(f32, f32); 3] = [(0.24, 0.52), (0.43, 0.71), (0.77, 0.31)];
const CHECK_MARK_WIDTH: f32 = 1.7;
const QUOTE_BAR_WIDTH: f32 = 3.0;
/// Space between the quote bar and the quoted text.
const QUOTE_PADDING: i8 = 16;
/// Opacity of the accent color used for the quote bar.
const QUOTE_BAR_TINT: f32 = 0.5;
const CODE_SIZE: f32 = 13.5;
/// Inline code size relative to the text around it (monospace looks larger).
const INLINE_CODE_SCALE: f32 = 0.9;
const CODE_LINE_HEIGHT: f32 = 20.0;
const CODE_RADIUS: u8 = 6;
/// Inner margin (x, y) of a code block.
const CODE_MARGIN: (i8, i8) = (14, 12);
const CODE_LABEL_SIZE: f32 = 11.0;
/// Offset of the language label from the code block's top-right corner.
const CODE_LABEL_OFFSET: (f32, f32) = (10.0, 6.0);
/// Opacity of the warning color behind text that has a note.
const NOTE_TINT: f32 = 0.35;
/// Total space a thematic break takes, its line in the middle.
const RULE_SPACING: f32 = 20.0;

/// Render blocks inside `depth` lists (bullets change shape with depth),
/// `gap` apart. List items use a smaller gap so nested lists stay tight.
fn render_blocks_at(ui: &mut Ui, blocks: &[Block], depth: usize, gap: f32) {
    for (index, block) in blocks.iter().enumerate() {
        if index > 0 {
            ui.add_space(gap_before(block, gap));
        }
        render_block(ui, block, depth);
    }
}

/// Draw one top-level block (for the clickable preview).
pub fn render_top_block(ui: &mut Ui, block: &Block) {
    render_block(ui, block, 0);
}

/// Space above a top-level block that follows another one.
pub fn top_gap(block: &Block) -> f32 {
    gap_before(block, BLOCK_GAP)
}

/// Space above a block: headings get more, so they sit close to their own text.
fn gap_before(block: &Block, gap: f32) -> f32 {
    match block {
        Block::Heading { level, .. } => HEADING_GAPS[heading_index(*level)],
        _ => gap,
    }
}

/// Index 0 to 5 into the per-level heading tables.
fn heading_index(level: u8) -> usize {
    usize::from(level).clamp(1, 6) - 1
}

/// Render one block with the widgets for its kind.
fn render_block(ui: &mut Ui, block: &Block, depth: usize) {
    match block {
        Block::Heading { level, content } => render_heading(ui, *level, content),
        // A paragraph that is only hidden comments takes no room.
        Block::Paragraph(content) if content.iter().all(|inline| matches!(inline, Inline::Note { .. })) => {}
        Block::Paragraph(content) => render_inlines(ui, content, &TextStyle::default()),
        Block::Code { info, code } => render_code(ui, info, code),
        Block::Quote(blocks) => render_quote(ui, blocks, depth),
        Block::List(list) => render_list(ui, list, depth),
        Block::ThematicBreak => {
            ui.add(egui::Separator::default().spacing(RULE_SPACING));
        }
    }
}

/// Heading text; levels 1 and 2 get a thin line underneath.
fn render_heading(ui: &mut Ui, level: u8, content: &[Inline]) {
    let size = HEADING_SIZES[heading_index(level)];
    let style = TextStyle { size, line_height: size * HEADING_LINE_FACTOR, strong: true, ..TextStyle::default() };
    render_inlines(ui, content, &style);
    if level <= RULED_HEADING_LEVEL {
        ui.add_space(HEADING_RULE_GAP);
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
        let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
        ui.painter().hline(rect.x_range(), rect.center().y, stroke);
    }
}

/// Code block in an outlined box, with its info string as a small label at the top right.
fn render_code(ui: &mut Ui, info: &str, code: &str) {
    let visuals = ui.visuals();
    let frame = egui::Frame::new()
        .fill(visuals.code_bg_color)
        .stroke(visuals.widgets.noninteractive.bg_stroke)
        .corner_radius(CODE_RADIUS)
        .inner_margin(Margin::symmetric(CODE_MARGIN.0, CODE_MARGIN.1));
    let response = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(code).monospace().size(CODE_SIZE).line_height(Some(CODE_LINE_HEIGHT)));
    });
    if !info.is_empty() {
        let corner = response.response.rect.right_top() + vec2(-CODE_LABEL_OFFSET.0, CODE_LABEL_OFFSET.1);
        let color = ui.visuals().weak_text_color();
        ui.painter().text(corner, Align2::RIGHT_TOP, info, FontId::proportional(CODE_LABEL_SIZE), color);
    }
}

/// Quoted blocks in softer text, with a bar in the accent color on the left.
fn render_quote(ui: &mut Ui, blocks: &[Block], depth: usize) {
    let weak_color = ui.visuals().weak_text_color();
    let bar_color = ui.visuals().hyperlink_color.gamma_multiply(QUOTE_BAR_TINT);
    let frame = egui::Frame::new().inner_margin(Margin { left: QUOTE_PADDING, ..Margin::ZERO });
    let response = frame.show(ui, |ui| {
        ui.visuals_mut().override_text_color = Some(weak_color);
        render_blocks_at(ui, blocks, depth, BLOCK_GAP);
    });
    let rect = response.response.rect;
    let x = rect.left() + QUOTE_BAR_WIDTH / 2.0;
    ui.painter().vline(x, rect.y_range(), Stroke::new(QUOTE_BAR_WIDTH, bar_color));
}

/// List items with a bullet, number, or checkbox in a column before each.
/// Done tasks are dimmed.
fn render_list(ui: &mut Ui, list: &List, depth: usize) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = LIST_ITEM_GAP;
        for (index, item) in list.items.iter().enumerate() {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = MARKER_GAP;
                match (item.task, list.start) {
                    (Some(checked), _) => paint_checkbox(ui, checked),
                    (None, Some(start)) => number_marker(ui, start + index as u64),
                    (None, None) => paint_bullet(ui, depth),
                }
                ui.vertical(|ui| {
                    if item.task == Some(true) {
                        ui.visuals_mut().override_text_color = Some(ui.visuals().weak_text_color());
                    }
                    render_blocks_at(ui, &item.blocks, depth + 1, LIST_ITEM_GAP);
                });
            });
        }
    });
}

/// Room for a list marker, one text line tall.
fn marker_rect(ui: &mut Ui) -> egui::Rect {
    ui.allocate_exact_size(vec2(MARKER_WIDTH, BODY_LINE_HEIGHT), Sense::hover()).0
}

/// Filled dot, then hollow dot, then square, repeating with nesting depth.
fn paint_bullet(ui: &mut Ui, depth: usize) {
    let center = marker_rect(ui).center();
    let color = ui.visuals().weak_text_color();
    let painter = ui.painter();
    match depth % 3 {
        0 => {
            painter.circle_filled(center, BULLET_RADIUS, color);
        }
        1 => {
            painter.circle_stroke(center, BULLET_RADIUS - 0.5, Stroke::new(1.3, color));
        }
        _ => {
            painter.rect_filled(egui::Rect::from_center_size(center, vec2(5.0, 5.0)), 1, color);
        }
    }
}

/// "3." in soft text, right-aligned in the marker column.
fn number_marker(ui: &mut Ui, number: u64) {
    let color = ui.visuals().weak_text_color();
    ui.allocate_ui_with_layout(vec2(MARKER_WIDTH, BODY_LINE_HEIGHT), Layout::right_to_left(Align::Center), |ui| {
        ui.label(RichText::new(format!("{number}.")).size(BODY_SIZE).color(color));
    });
}

/// Read-only task checkbox: an outline, or filled with the accent and a check mark.
fn paint_checkbox(ui: &mut Ui, checked: bool) {
    let rect = marker_rect(ui);
    let center = pos2(rect.left() + CHECKBOX_SIZE / 2.0, rect.center().y);
    let square = egui::Rect::from_center_size(center, vec2(CHECKBOX_SIZE, CHECKBOX_SIZE));
    let visuals = ui.visuals();
    let painter = ui.painter();
    if checked {
        painter.rect_filled(square, CHECKBOX_RADIUS, visuals.hyperlink_color);
        let points: Vec<_> = CHECK_MARK.iter().map(|&(x, y)| square.lerp_inside(vec2(x, y))).collect();
        painter.line(points, Stroke::new(CHECK_MARK_WIDTH, check_mark_color(visuals)));
    } else {
        let stroke = Stroke::new(CHECKBOX_STROKE, visuals.weak_text_color());
        painter.rect_stroke(square, CHECKBOX_RADIUS, stroke, StrokeKind::Inside);
    }
}

/// The check mark uses the page color, so it reads on the accent in both themes.
fn check_mark_color(visuals: &egui::Visuals) -> Color32 {
    visuals.text_edit_bg_color()
}

/// Text formatting accumulated while walking nested inlines.
#[derive(Clone)]
#[expect(clippy::struct_excessive_bools, reason = "independent text styles that combine freely, e.g. bold + italic")]
struct TextStyle {
    /// Font size in points.
    size: f32,
    /// Height of one line of text in points.
    line_height: f32,
    strong: bool,
    italic: bool,
    strikethrough: bool,
    code: bool,
    /// Target URL when the text is part of a link.
    link: Option<String>,
    /// What a note says about this text (highlighted, shown on hover).
    note: Option<String>,
}

impl Default for TextStyle {
    fn default() -> Self {
        TextStyle {
            size: BODY_SIZE,
            line_height: BODY_LINE_HEIGHT,
            strong: false,
            italic: false,
            strikethrough: false,
            code: false,
            link: None,
            note: None,
        }
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
        // Spans touch; the line height alone sets the space between lines.
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
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
    // egui draws strike and underline lines from the line box, so a tall
    // line height pushes them too low: those spans keep the font's own height.
    let decorated = style.strikethrough || style.link.as_ref().is_some_and(|url| !is_openable_url(url));
    let line_height = (!decorated).then_some(style.line_height);
    let size = if style.code { style.size * INLINE_CODE_SCALE } else { style.size };
    let mut rich = RichText::new(text).size(size).line_height(line_height);
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
    if style.note.is_some() {
        rich = rich.background_color(ui.visuals().warn_fg_color.gamma_multiply(NOTE_TINT));
    }
    let note_tip = |response: egui::Response| match &style.note {
        Some(note) => response.on_hover_text(format!("Note: {note}")),
        None => response,
    };
    match &style.link {
        Some(url) if is_openable_url(url) => {
            note_tip(ui.hyperlink_to(rich, url).on_hover_text(url));
        }
        // Local or unknown targets are shown, not opened (see specs/03).
        Some(url) => {
            note_tip(
                ui.label(rich.underline()).on_hover_text(format!("{url} (not opened: only web and mail links open)")),
            );
        }
        None => {
            note_tip(ui.label(rich));
        }
    }
}

/// Highlight the end of the spans so far that matches `quote` (the text a
/// note is about) and attach the note to it, splitting a span if needed.
fn mark_note(spans: &mut Vec<Span>, quote: &str, note: &str) {
    let shown: String = spans
        .iter()
        .filter_map(|span| if let Span::Text(text, _) = span { Some(text.as_str()) } else { None })
        .collect();
    let normalize = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    if quote.is_empty() || !normalize(&shown).ends_with(&normalize(quote)) {
        return;
    }
    let mut remaining = quote.trim_end().chars().count();
    let mut index = spans.len();
    while remaining > 0 && index > 0 {
        index -= 1;
        let Span::Text(text, style) = &mut spans[index] else {
            continue;
        };
        let length = text.chars().count();
        if length <= remaining {
            style.note = Some(note.to_string());
            remaining -= length;
        } else {
            let split_at = text.char_indices().nth(length - remaining).map_or(text.len(), |(offset, _)| offset);
            let tail = text.split_off(split_at);
            let marked = TextStyle { note: Some(note.to_string()), ..style.clone() };
            spans.insert(index + 1, Span::Text(tail, marked));
            remaining = 0;
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
            Inline::Note { quote, text } => mark_note(spans, quote, text),
        }
    }
}
