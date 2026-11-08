//! Drag explorer rows up and down to reorder a list: the root folders in
//! Files and the sessions in Terminals.

use eframe::egui::{self, Ui};

/// How close to the top or bottom edge of the list the pointer has to be
/// for the list to scroll while a row is dragged, in points.
const SCROLL_EDGE: f32 = 24.0;
/// How far the list scrolls per frame while the pointer is at an edge.
const SCROLL_STEP: f32 = 6.0;

/// One list's rows as they are drawn: afterwards [`ListDrag::finish`]
/// shows where a dragged row would land and reports the move on drop.
#[derive(Debug, Default)]
pub(super) struct ListDrag {
    /// Each item, in drawing order: where it starts (its row's top) and
    /// ends (below anything unfolded under it), and its row's middle.
    items: Vec<ItemSpan>,
    /// The item being dragged, and whether it was dropped this frame.
    dragged: Option<(usize, bool)>,
}

/// Where one item of the list is on screen, in points.
#[derive(Clone, Copy, Debug)]
struct ItemSpan {
    top: f32,
    row_middle: f32,
    bottom: f32,
}

impl ListDrag {
    /// Note the next item, drawn from `top` down to the cursor. `row` is its
    /// draggable row (see [`draggable`]), or `None` while it shows a name
    /// field instead.
    pub(super) fn add(&mut self, ui: &Ui, top: f32, row: Option<&egui::Response>) {
        let index = self.items.len();
        let bottom = ui.cursor().top();
        let row_middle = row.map_or(f32::midpoint(top, bottom), |row| row.rect.center().y);
        self.items.push(ItemSpan { top, row_middle, bottom });
        let Some(row) = row else { return };
        if row.drag_stopped() {
            self.dragged = Some((index, true));
        } else if row.dragged() {
            self.dragged = Some((index, false));
            // Fade the row in its place, so it reads as being moved.
            let fade = ui.visuals().panel_fill.gamma_multiply(0.5);
            ui.painter().rect_filled(row.rect, 0, fade);
        }
    }

    /// After the last item: while a row is dragged, a line where it would
    /// land (and the list scrolls at its edges); on the drop, the move as
    /// `(from, to)`, `to` being the item it goes before (the item count for
    /// the end). Nothing when the row would stay where it is.
    pub(super) fn finish(self, ui: &Ui) -> Option<(usize, usize)> {
        let (from, dropped) = self.dragged?;
        let pointer = ui.input(|input| input.pointer.latest_pos())?;
        let to = drop_slot(&self.items, pointer.y);
        let moves = to != from && to != from + 1;
        if dropped {
            return moves.then_some((from, to));
        }
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        scroll_at_edges(ui, pointer.y);
        if moves {
            // Halfway into the gap between two items.
            let half_gap = ui.spacing().item_spacing.y / 2.0;
            let y = match self.items.get(to) {
                Some(item) => item.top - half_gap,
                None => self.items.last().map_or(pointer.y, |item| item.bottom - half_gap),
            };
            let stroke = egui::Stroke::new(2.0, ui.visuals().hyperlink_color);
            ui.painter().hline(ui.clip_rect().x_range(), y, stroke);
        }
        None
    }
}

/// `row` that can also be dragged, to reorder its list.
pub(super) fn draggable(row: &egui::Response) -> egui::Response {
    row.interact(egui::Sense::drag())
}

/// Where a row dropped at height `y` goes: before the first item whose row
/// is below the pointer, or at the end. Pointing into the unfolded tree of
/// an item counts as past its row, so the drop lands after it.
fn drop_slot(items: &[ItemSpan], y: f32) -> usize {
    items.iter().position(|item| y < item.row_middle).unwrap_or(items.len())
}

/// Scroll the list when the pointer is near its top or bottom edge, so a
/// row can be dragged to a place that is out of view.
fn scroll_at_edges(ui: &Ui, y: f32) {
    let visible = ui.clip_rect();
    let delta = if y < visible.top() + SCROLL_EDGE {
        SCROLL_STEP
    } else if y > visible.bottom() - SCROLL_EDGE {
        -SCROLL_STEP
    } else {
        return;
    };
    ui.scroll_with_delta(egui::vec2(0.0, delta));
    ui.ctx().request_repaint();
}

/// Move the item at `from` to go before the item that was at `to` (the
/// length moves it to the end), as [`ListDrag::finish`] reports a drop.
pub fn move_item<T>(items: &mut Vec<T>, from: usize, to: usize) {
    if from >= items.len() || to > items.len() {
        return;
    }
    let item = items.remove(from);
    // Taking the item out shifts the ones after it up by one.
    let to = if to > from { to - 1 } else { to };
    items.insert(to, item);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moved(from: usize, to: usize) -> Vec<char> {
        let mut items = vec!['a', 'b', 'c', 'd'];
        move_item(&mut items, from, to);
        items
    }

    #[test]
    fn moves_down_and_up() {
        assert_eq!(moved(0, 2), ['b', 'a', 'c', 'd']);
        assert_eq!(moved(0, 4), ['b', 'c', 'd', 'a']);
        assert_eq!(moved(3, 0), ['d', 'a', 'b', 'c']);
        assert_eq!(moved(2, 1), ['a', 'c', 'b', 'd']);
    }

    #[test]
    fn ignores_places_outside_the_list() {
        assert_eq!(moved(4, 0), ['a', 'b', 'c', 'd']);
        assert_eq!(moved(0, 5), ['a', 'b', 'c', 'd']);
    }

    /// Draw rows "a" to "d" (dragging reorders them) for each pointer
    /// position in `steps`, pressing or releasing the button where it says
    /// so, and return the rows' order afterwards.
    fn drag_rows(steps: &[(egui::Pos2, Option<bool>)]) -> Vec<char> {
        use eframe::egui::{Event, PointerButton, RawInput};
        let ctx = egui::Context::default();
        let mut rows = vec!['a', 'b', 'c', 'd'];
        for (frame, (pos, button)) in steps.iter().enumerate() {
            let mut input = RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 400.0))),
                time: Some(f64::from(u16::try_from(frame).unwrap_or(0)) * 0.016),
                ..Default::default()
            };
            input.events.push(Event::PointerMoved(*pos));
            if let Some(pressed) = *button {
                input.events.push(Event::PointerButton {
                    pos: *pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::default(),
                });
            }
            let mut output = ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let mut drag = ListDrag::default();
                    for row in &rows {
                        let top = ui.cursor().top();
                        let response = draggable(&ui.add(egui::Button::selectable(false, row.to_string())));
                        drag.add(ui, top, Some(&response));
                    }
                    if let Some((from, to)) = drag.finish(ui) {
                        move_item(&mut rows, from, to);
                    }
                });
            });
            output.textures_delta.clear();
        }
        rows
    }

    /// Press at `from`, move to `to` in small steps like a hand, release.
    fn drag_steps(from: egui::Pos2, to: egui::Pos2) -> Vec<(egui::Pos2, Option<bool>)> {
        let mut steps = vec![(from, None), (from, None), (from, Some(true))];
        steps.extend((1..=20_u8).map(|n| (from + (to - from) * f32::from(n) / 20.0, None)));
        steps.push((to, Some(false)));
        steps
    }

    /// The middle of each row "a" to "d", from drawing them once.
    fn row_centers() -> Vec<egui::Pos2> {
        let ctx = egui::Context::default();
        let mut centers = Vec::new();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                for row in ['a', 'b', 'c', 'd'] {
                    centers.push(ui.add(egui::Button::selectable(false, row.to_string())).rect.center());
                }
            });
        });
        output.textures_delta.clear();
        centers
    }

    #[test]
    fn dragging_a_row_down_and_up_moves_it() {
        let centers = row_centers();
        let (a, c, d) = (centers[0], centers[2], centers[3]);
        // Onto the lower half of "c": after it.
        let below_c = egui::pos2(c.x, c.y + 3.0);
        assert_eq!(drag_rows(&drag_steps(a, below_c)), ['b', 'c', 'a', 'd']);
        // From "d" onto the upper half of "a": first.
        let above_a = egui::pos2(a.x, a.y - 3.0);
        assert_eq!(drag_rows(&drag_steps(d, above_a)), ['d', 'a', 'b', 'c']);
    }

    #[test]
    fn a_click_moves_nothing() {
        let a = row_centers()[0];
        let rows = drag_rows(&[(a, None), (a, Some(true)), (a, Some(false))]);
        assert_eq!(rows, ['a', 'b', 'c', 'd']);
    }

    #[test]
    fn a_drop_lands_before_the_first_row_below_the_pointer() {
        // Two items: the first has an unfolded tree from 30 to 100.
        let items = [
            ItemSpan { top: 0.0, row_middle: 10.0, bottom: 100.0 },
            ItemSpan { top: 100.0, row_middle: 110.0, bottom: 120.0 },
        ];
        assert_eq!(drop_slot(&items, 5.0), 0);
        assert_eq!(drop_slot(&items, 60.0), 1, "inside the first item's tree");
        assert_eq!(drop_slot(&items, 115.0), 2);
    }
}
