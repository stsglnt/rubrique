//! Tab groups: with more than one folder in the explorer, tabs are grouped
//! by the folder they belong to. Each folder has a color (a dot on its row
//! in the explorer); its group of tabs starts with a chip in that color
//! and each of its tabs has a thin line in it. A click on a chip folds the
//! group down to the chip (and the active tab, if it is in there).
//!
//! Tabs stay next to their group: a new tab opens at the end of its group,
//! and the tabs restored at startup are sorted into groups.

use eframe::egui::{self, Color32, Sense, Ui, WidgetInfo, pos2, vec2};

use super::{OpenFile, RubriqueApp};
use crate::theme::style::group_color;

/// Height of a group chip: the same as the Preview | Source switch at the
/// other end of the strip.
const CHIP_HEIGHT: f32 = 22.0;
/// Space left and right of a chip's contents.
const CHIP_PADDING: f32 = 8.0;
/// Space around a chip in the tab strip.
const CHIP_MARGIN: f32 = 6.0;
/// Corner radius of a chip: the widget radius, like the buttons around it
/// (a pill was the only fully round shape in the window).
const CHIP_RADIUS: u8 = 5;
/// Radius of the folder's color dot at the start of a chip, the same dot
/// as on the folder's row in the explorer.
const CHIP_DOT: f32 = 3.0;
/// Space between the dot and the folder's name.
const CHIP_DOT_GAP: f32 = 6.0;
/// How strongly a chip is filled with its group color; a folded chip,
/// which stands for its tabs, a little more.
const CHIP_TINT: f32 = 0.16;
const FOLDED_CHIP_TINT: f32 = 0.26;
/// How much stronger the fill gets under the pointer.
const HOVER_TINT: f32 = 1.6;

/// What a chip asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ChipClick {
    /// The group whose fold state changes.
    pub group: Option<usize>,
}

impl RubriqueApp {
    /// True when tabs are grouped: more than one folder in the explorer.
    pub(super) fn tabs_grouped(&self) -> bool {
        self.roots.len() > 1
    }

    /// The group of `file`: the index of the explorer folder it is in (the
    /// deepest one when folders nest). `None` for files outside them all,
    /// such as scratches.
    pub(super) fn group_of(&self, file: &OpenFile) -> Option<usize> {
        let place = match file {
            OpenFile::Terminal { terminal } => terminal.file.as_deref().unwrap_or_else(|| terminal.folder()),
            other => other.path()?,
        };
        self.roots
            .iter()
            .enumerate()
            .filter(|(_, root)| place.starts_with(&root.path))
            .max_by_key(|(_, root)| root.path.components().count())
            .map(|(index, _)| index)
    }

    /// The color of a group; `None` for the files outside every folder.
    pub(super) fn group_color_of(&self, group: Option<usize>) -> Option<Color32> {
        group.filter(|_| self.tabs_grouped()).map(group_color)
    }

    /// Where a new tab for `file` goes: right after the last tab of its
    /// group, or where its group would start. Without groups, right after
    /// the active tab.
    pub(super) fn new_tab_index(&self, file: &OpenFile) -> usize {
        if !self.tabs_grouped() {
            return self.active_tab + 1;
        }
        let group = group_order(self.group_of(file));
        let groups: Vec<usize> =
            (0..self.tabs.len()).map(|index| group_order(self.group_of(self.tab(index)))).collect();
        match groups.iter().rposition(|&other| other == group) {
            Some(last) => last + 1,
            None => groups.iter().position(|&other| other > group).unwrap_or(groups.len()),
        }
    }

    /// Put the tabs in group order (keeping their order inside a group), so
    /// each group is in one piece. The active tab stays active.
    pub(super) fn sort_tabs_into_groups(&mut self) {
        if !self.tabs_grouped() || self.tabs.is_empty() {
            return;
        }
        let mut order: Vec<usize> = (0..self.tabs.len()).collect();
        order.sort_by_key(|&index| group_order(self.group_of(self.tab(index))));
        let mut slots: Vec<Option<OpenFile>> = std::mem::take(&mut self.tabs).into_iter().map(Some).collect();
        self.tabs = order.iter().map(|&index| slots[index].take().unwrap_or(OpenFile::Nothing)).collect();
        self.active_tab = order.iter().position(|&index| index == self.active_tab).unwrap_or(0);
    }

    /// The chip that starts a group: the folder's color dot and its name
    /// (and how many tabs it has while folded) on a soft fill in the group
    /// color. Returns a click on it.
    pub(super) fn group_chip(&self, ui: &mut Ui, group: Option<usize>, folded: bool) -> Option<ChipClick> {
        let name = group.and_then(|index| self.roots.get(index)).map_or("Other", |root| root.name.as_str());
        let count = (0..self.tabs.len()).filter(|&index| self.group_of(self.tab(index)) == group).count();
        let text = if folded { format!("{name} · {count}") } else { name.to_string() };
        let color = self.group_color_of(group).unwrap_or_else(|| ui.visuals().weak_text_color());
        let font = egui::TextStyle::Small.resolve(ui.style());
        let galley = ui.painter().layout_no_wrap(text.clone(), font, color);
        let dot_width = CHIP_DOT * 2.0 + CHIP_DOT_GAP;
        let chip_width = CHIP_PADDING * 2.0 + dot_width + galley.size().x;
        let size = vec2(chip_width + CHIP_MARGIN * 2.0, super::tabs::TAB_HEIGHT);
        let (slot, response) = ui.allocate_exact_size(size, Sense::click());
        response.widget_info(|| WidgetInfo::labeled(egui::Role::Button, true, &text));
        let chip = egui::Rect::from_center_size(slot.center(), vec2(chip_width, CHIP_HEIGHT));
        let tint = if folded { FOLDED_CHIP_TINT } else { CHIP_TINT };
        let fill = color.gamma_multiply(if response.hovered() { tint * HOVER_TINT } else { tint });
        ui.painter().rect_filled(chip, CHIP_RADIUS, fill);
        let dot = pos2(chip.left() + CHIP_PADDING + CHIP_DOT, chip.center().y);
        ui.painter().circle_filled(dot, CHIP_DOT, color);
        let text_pos = pos2(chip.left() + CHIP_PADDING + dot_width, chip.center().y - galley.size().y / 2.0);
        ui.painter().galley(text_pos, galley, color);
        let tip = if folded { "Show this group's tabs" } else { "Fold this group's tabs" };
        response.on_hover_text(tip).clicked().then_some(ChipClick { group })
    }

    /// Fold or unfold a group's tabs.
    pub(super) fn toggle_group(&mut self, group: Option<usize>) {
        let key = group_key(self, group);
        if let Some(index) = self.folded_groups.iter().position(|folded| *folded == key) {
            self.folded_groups.remove(index);
        } else {
            self.folded_groups.push(key);
        }
    }

    /// True when `group` is folded.
    pub(super) fn group_folded(&self, group: Option<usize>) -> bool {
        let key = group_key(self, group);
        self.folded_groups.contains(&key)
    }
}

/// Where a group sorts: by folder, the files outside every folder last.
fn group_order(group: Option<usize>) -> usize {
    group.unwrap_or(usize::MAX)
}

/// What a folded group is remembered by: its folder (so removing another
/// folder does not fold the wrong group), or an empty path for "Other".
fn group_key(app: &RubriqueApp, group: Option<usize>) -> std::path::PathBuf {
    group.and_then(|index| app.roots.get(index)).map_or_else(Default::default, |root| root.path.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_outside_every_folder_sort_last() {
        assert!(group_order(Some(3)) < group_order(None));
        assert!(group_order(Some(0)) < group_order(Some(1)));
    }
}
