//! Draw an image file, or a card for a video file, in the page.

use std::path::Path;

use eframe::egui::{self, Color32, RichText, Sense, Stroke, Ui, load::ImagePoll, load::SizeHint, vec2};

use crate::media::{format_name, image_uri, size_text};

/// Space between the page edge and a fitted image.
const IMAGE_MARGIN: f32 = 24.0;
/// Size of the squares in the checkerboard behind see-through pixels.
const CHECKER: f32 = 10.0;
/// Opacity of the darker checkerboard squares.
const CHECKER_TINT: f32 = 0.06;
/// Radius of the big play circle on a video card.
const PLAY_RADIUS: f32 = 34.0;
/// Width of the play circle's ring.
const PLAY_RING: f32 = 3.0;

/// What the user did on an image or video page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaEvent {
    /// Switch an image between fitted to the page and its own size.
    ToggleSize,
    /// Open the video in the system's video player.
    PlayInPlayer,
    /// Open the video in the web browser.
    PlayInBrowser,
}

/// An image's width and height in pixels, once it has loaded.
pub fn image_pixels(ctx: &egui::Context, path: &Path) -> Option<[usize; 2]> {
    match ctx.try_load_image(&image_uri(path), SizeHint::default()) {
        Ok(ImagePoll::Ready { image }) => Some(image.size),
        _ => None,
    }
}

/// Draw the image at `path`: shrunk to fit the page (never enlarged), or
/// at its own size with scroll bars when `actual_size` is set. A click
/// switches between the two.
pub fn show_image(ui: &mut Ui, path: &Path, actual_size: bool) -> Option<MediaEvent> {
    let uri = image_uri(path);
    let image = egui::Image::new(uri).fit_to_original_size(1.0).sense(Sense::click());
    let available = ui.available_size() - vec2(IMAGE_MARGIN, IMAGE_MARGIN) * 2.0;
    if let Err(error) = image.load_for_size(ui.ctx(), available) {
        egui::Frame::new().inner_margin(IMAGE_MARGIN).show(ui, |ui| {
            ui.heading(crate::explorer::paths::name_text(path));
            ui.label(format!("Cannot show this image: {error}"));
        });
        return None;
    }
    let response = if actual_size {
        egui::ScrollArea::both()
            .id_salt(("image-scroll", path))
            .auto_shrink([false, false])
            .show(ui, |ui| egui::Frame::new().inner_margin(IMAGE_MARGIN).show(ui, |ui| add_image(ui, image)).inner)
            .inner
    } else {
        let image = image.max_size(available.max(vec2(1.0, 1.0)));
        ui.centered_and_justified(|ui| add_image(ui, image)).inner
    };
    let tip = if actual_size { "Click to fit the page" } else { "Click for actual size" };
    response.on_hover_text(tip).clicked().then_some(MediaEvent::ToggleSize)
}

/// The image with a soft checkerboard behind it, so see-through parts show.
fn add_image(ui: &mut Ui, image: egui::Image<'_>) -> egui::Response {
    let background = ui.painter().add(egui::Shape::Noop);
    let response = ui.add(image);
    let rect = response.rect;
    let dark = ui.visuals().text_color().gamma_multiply(CHECKER_TINT);
    let mut squares = Vec::new();
    let mut y = rect.top();
    let mut row = 0_u32;
    while y < rect.bottom() {
        let mut x = rect.left() + if row.is_multiple_of(2) { 0.0 } else { CHECKER };
        while x < rect.right() {
            let square = egui::Rect::from_min_size(egui::pos2(x, y), vec2(CHECKER, CHECKER)).intersect(rect);
            squares.push(egui::Shape::rect_filled(square, 0, dark));
            x += CHECKER * 2.0;
        }
        y += CHECKER;
        row += 1;
    }
    ui.painter().set(background, egui::Shape::Vec(squares));
    response
}

/// A card for the video at `path` (`bytes` long): rubrique cannot decode
/// video, so it offers the system player and the browser.
pub fn show_video(ui: &mut Ui, path: &Path, bytes: u64) -> Option<MediaEvent> {
    let mut event = None;
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() / 2.0 - 140.0).max(24.0));
        let (rect, play) = ui.allocate_exact_size(vec2(PLAY_RADIUS * 2.0, PLAY_RADIUS * 2.0), Sense::click());
        paint_play(ui, rect, play.hovered());
        if play.on_hover_text("Play in your video player").clicked() {
            event = Some(MediaEvent::PlayInPlayer);
        }
        ui.add_space(14.0);
        ui.label(RichText::new(crate::explorer::paths::name_text(path)).heading());
        ui.label(RichText::new(format!("{} video · {}", format_name(path), size_text(bytes))).weak());
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            // Center the two buttons as a group.
            let width = 300.0_f32.min(ui.available_width());
            ui.add_space((ui.available_width() - width) / 2.0);
            if ui.button("Play in video player").clicked() {
                event = Some(MediaEvent::PlayInPlayer);
            }
            if ui.button("Play in browser").on_hover_text("Browsers play MP4, WebM and Ogg").clicked() {
                event = Some(MediaEvent::PlayInBrowser);
            }
        });
        ui.add_space(10.0);
        ui.label(RichText::new("rubrique shows videos but does not play them inside the window.").weak().small());
    });
    event
}

/// A ring with a triangle in the accent color.
fn paint_play(ui: &Ui, rect: egui::Rect, hovered: bool) {
    let visuals = ui.visuals();
    let accent = visuals.hyperlink_color;
    let painter = ui.painter();
    let center = rect.center();
    let fill = if hovered { accent.gamma_multiply(0.15) } else { Color32::TRANSPARENT };
    painter.circle(center, PLAY_RADIUS - PLAY_RING, fill, Stroke::new(PLAY_RING, visuals.weak_text_color()));
    let size = PLAY_RADIUS * 0.42;
    let points = vec![center + vec2(-size * 0.55, -size), center + vec2(size, 0.0), center + vec2(-size * 0.55, size)];
    painter.add(egui::Shape::convex_polygon(points, accent, Stroke::NONE));
}
