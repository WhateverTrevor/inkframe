//! Layer folders and layer colours, as in Animate's timeline.
//!
//! A folder is a layer with `folder` set: it draws nothing and exists to
//! hold the layers beneath it, which name it in `in_folder`. Folders can
//! hold folders. A folder's members always sit directly below it in the
//! stack, so the timeline can show them indented under it and restacking
//! the folder takes them along; [`Document::tidy_folders`] restores that
//! after any change to the stack.
//!
//! Every layer also has a colour, which the timeline uses for its bar and
//! the stage for its selection outline, so a layer is easy to pick out in
//! both. Unless one is chosen, it comes from [`TAG_COLORS`] by layer id.

use crate::{Color, Document, Layer, LayerId, TimelineId, Transform};

/// The colours layers get unless one is chosen: distinct at a glance on a
/// dark timeline, in the order new layers take them.
pub const TAG_COLORS: [Color; 8] = [
    Color::rgb(0x4f, 0x9d, 0xff), // blue
    Color::rgb(0xff, 0x6b, 0x6b), // red
    Color::rgb(0x5c, 0xd6, 0x8a), // green
    Color::rgb(0xff, 0xb8, 0x4d), // orange
    Color::rgb(0xb8, 0x7c, 0xff), // purple
    Color::rgb(0x4d, 0xd8, 0xe0), // cyan
    Color::rgb(0xff, 0x7c, 0xc8), // pink
    Color::rgb(0xd6, 0xd0, 0x5c), // yellow
];

impl Layer {
    /// The layer's colour: the one chosen for it, or one from
    /// [`TAG_COLORS`].
    pub fn tag_color(&self) -> Color {
        self.color.unwrap_or(TAG_COLORS[(self.id.0 % TAG_COLORS.len() as u64) as usize])
    }
}

impl Document {
    /// Adds a folder holding `layers` (any that are in `timeline`), placed
    /// where the topmost of them was, or on top of the stack if there are
    /// none. The folder joins whatever folder that topmost layer was in.
    /// Returns the folder's id.
    pub fn add_folder(&mut self, timeline: TimelineId, layers: &[LayerId]) -> Option<LayerId> {
        let name = self.next_folder_name(timeline);
        let stack = self.layers_of(timeline);
        let topmost = stack.iter().rposition(|l| layers.contains(&l.id));
        let outer = topmost.and_then(|i| stack[i].in_folder);
        let id = self.add_layer_to(timeline, name, Vec::new(), 0, Transform::default())?;
        let folder = self.layer_mut(id)?;
        folder.folder = true;
        folder.in_folder = outer;
        if let Some(index) = topmost {
            self.move_layer_to(id, index + 1);
        }
        for &member in layers {
            if member != id
                && let Some(layer) = self.layer_mut(member)
            {
                layer.in_folder = Some(id);
            }
        }
        self.tidy_folders(timeline);
        Some(id)
    }

    /// Takes `layer` out of its folder, or puts it in `folder`, moving it
    /// to the top of the folder's contents, or to just below the folder it
    /// leaves. A folder can't go inside itself.
    pub fn set_folder(&mut self, layer: LayerId, folder: Option<LayerId>) {
        let Some(timeline) = self.timeline_of(layer) else { return };
        if let Some(folder) = folder
            && (folder == layer
                || self.timeline_of(folder) != Some(timeline)
                || !self.layer(folder).is_some_and(|f| f.folder)
                || self.is_inside(folder, layer))
        {
            return;
        }
        let old = self.layer(layer).and_then(|l| l.in_folder);
        if old == folder {
            return;
        }
        if let Some(l) = self.layer_mut(layer) {
            l.in_folder = folder;
        }
        match folder {
            // Just under the folder, so at the top of its contents.
            Some(folder) => {
                if let Some(index) = self.layer_index(folder) {
                    let from = self.layer_index(layer).unwrap_or(0);
                    self.move_layer_to(layer, if from < index { index - 1 } else { index });
                }
            }
            // Just below the block of the folder it left.
            None => {
                if let Some(old) = old {
                    let bottom = self.folder_block(old).last().copied();
                    if let Some(index) = bottom.and_then(|b| self.layer_index(b)) {
                        let from = self.layer_index(layer).unwrap_or(0);
                        self.move_layer_to(layer, if from < index { index - 1 } else { index });
                    }
                }
            }
        }
        self.tidy_folders(timeline);
    }

    /// Moves `layer` (with its contents, if it's a folder) to just above
    /// `above_of` in the stack, or to the bottom if that's `None`, and into
    /// `folder` (or out of any). This is a drop in the timeline. Returns
    /// false, changing nothing, if the move makes no sense: onto itself, or
    /// a folder into itself.
    pub fn place_layer(
        &mut self,
        layer: LayerId,
        above_of: Option<LayerId>,
        folder: Option<LayerId>,
    ) -> bool {
        let Some(timeline) = self.timeline_of(layer) else { return false };
        let inside_moved = |doc: &Self, id: LayerId| id == layer || doc.is_inside(id, layer);
        if above_of.is_some_and(|id| inside_moved(self, id) || self.timeline_of(id) != Some(timeline)) {
            return false;
        }
        if let Some(folder) = folder
            && (inside_moved(self, folder)
                || self.timeline_of(folder) != Some(timeline)
                || !self.layer(folder).is_some_and(|f| f.folder))
        {
            return false;
        }
        if let Some(l) = self.layer_mut(layer) {
            l.in_folder = folder;
        }
        let Some(stack) = self.layers_of_mut(timeline) else { return false };
        let Some(from) = stack.iter().position(|l| l.id == layer) else { return false };
        let moved = stack.remove(from);
        let to = above_of.and_then(|id| stack.iter().position(|l| l.id == id)).map_or(0, |i| i + 1);
        stack.insert(to, moved);
        self.tidy_folders(timeline);
        true
    }

    /// Moves a layer up (`delta > 0`) or down past its neighbours within
    /// its folder (or the top level), stepping over whole folders rather
    /// than into them.
    pub fn move_among_siblings(&mut self, layer: LayerId, delta: isize) {
        let Some(timeline) = self.timeline_of(layer) else { return };
        let folder = self.layer(layer).and_then(|l| l.in_folder);
        // Top first, as the timeline lists them.
        let siblings: Vec<LayerId> =
            self.layers_of(timeline).iter().rev().filter(|l| l.in_folder == folder).map(|l| l.id).collect();
        let Some(at) = siblings.iter().position(|&id| id == layer) else { return };
        let target = at.saturating_add_signed(-delta).min(siblings.len() - 1);
        if target == at {
            return;
        }
        let neighbour = siblings[target];
        let above_of = if target < at {
            // Up: just above the neighbour, whose header tops its block.
            Some(neighbour)
        } else {
            // Down: just above whatever is below the neighbour's block.
            let bottom = *self.folder_block(neighbour).last().unwrap_or(&neighbour);
            let index = self.layer_index(bottom).unwrap_or(0);
            index.checked_sub(1).map(|i| self.layers_of(timeline)[i].id)
        };
        self.place_layer(layer, above_of, folder);
    }

    /// Whether `layer` is inside `folder`, at any depth.
    pub fn is_inside(&self, layer: LayerId, folder: LayerId) -> bool {
        let mut current = self.layer(layer).and_then(|l| l.in_folder);
        // Bounded, in case a damaged file has a loop.
        for _ in 0..64 {
            match current {
                Some(id) if id == folder => return true,
                Some(id) => current = self.layer(id).and_then(|l| l.in_folder),
                None => return false,
            }
        }
        false
    }

    /// How many folders deep a layer is: 0 for one in no folder.
    pub fn folder_depth(&self, layer: &Layer) -> usize {
        std::iter::successors(layer.in_folder, |&id| self.layer(id).and_then(|l| l.in_folder))
            .take(64)
            .count()
    }

    /// A folder and everything in it at any depth, top first: the block of
    /// the stack that moves together.
    pub fn folder_block(&self, folder: LayerId) -> Vec<LayerId> {
        let Some(timeline) = self.timeline_of(folder) else { return Vec::new() };
        let mut block = vec![folder];
        block.extend(
            self.layers_of(timeline).iter().rev().filter(|l| self.is_inside(l.id, folder)).map(|l| l.id),
        );
        block
    }

    /// Restores the rule that a folder's contents sit directly below it,
    /// keeping everything else in order, and forgets folders that are gone
    /// (or that aren't folders, or that would contain themselves).
    pub fn tidy_folders(&mut self, timeline: TimelineId) {
        let folders: Vec<LayerId> =
            self.layers_of(timeline).iter().filter(|l| l.folder).map(|l| l.id).collect();
        let ids: Vec<LayerId> = self.layers_of(timeline).iter().map(|l| l.id).collect();
        for &id in &ids {
            let broken = self
                .layer(id)
                .and_then(|l| l.in_folder)
                .is_some_and(|f| !folders.contains(&f) || f == id || self.is_inside(f, id));
            if broken && let Some(layer) = self.layer_mut(id) {
                layer.in_folder = None;
            }
        }

        // Rebuild the stack top first: each layer, then (if it's a folder)
        // its contents in their current order.
        let Some(stack) = self.layers_of_mut(timeline) else { return };
        let mut remaining: Vec<Option<Layer>> = std::mem::take(stack).into_iter().rev().map(Some).collect();
        let mut ordered = Vec::with_capacity(remaining.len());
        fn place(at: usize, remaining: &mut [Option<Layer>], ordered: &mut Vec<Layer>) {
            let Some(layer) = remaining[at].take() else { return };
            let id = layer.id;
            let is_folder = layer.folder;
            ordered.push(layer);
            if is_folder {
                for i in 0..remaining.len() {
                    if remaining[i].as_ref().is_some_and(|l| l.in_folder == Some(id)) {
                        place(i, remaining, ordered);
                    }
                }
            }
        }
        for i in 0..remaining.len() {
            // Top-level layers start a block; members are placed by their
            // folder.
            if remaining[i].as_ref().is_some_and(|l| l.in_folder.is_none()) {
                place(i, &mut remaining, &mut ordered);
            }
        }
        // Anything left over sits in a folder cycle the check above missed;
        // keep it rather than lose it.
        ordered.extend(remaining.into_iter().flatten().map(|mut l| {
            l.in_folder = None;
            l
        }));
        ordered.reverse();
        if let Some(stack) = self.layers_of_mut(timeline) {
            *stack = ordered;
        }
        if timeline == TimelineId::Main {
            self.keep_camera_on_top();
        }
    }

    fn next_folder_name(&self, timeline: TimelineId) -> String {
        let taken = |name: &str| self.layers_of(timeline).iter().any(|l| l.name == name);
        (1..).map(|n| format!("Folder {n}")).find(|name| !taken(name)).expect("some number is free")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A document with layers A (bottom), B, C, D (top).
    fn stack() -> (Document, [LayerId; 4]) {
        let mut doc = Document::default();
        let ids = ["A", "B", "C", "D"].map(|name| doc.add_layer(name, Vec::new(), 0, Transform::default()));
        (doc, ids)
    }

    /// Names top first, members indented by their folder depth.
    fn names(doc: &Document) -> Vec<String> {
        doc.layers.iter().rev().map(|l| format!("{}{}", "  ".repeat(doc.folder_depth(l)), l.name)).collect()
    }

    #[test]
    fn a_new_folder_takes_the_place_of_its_topmost_layer() {
        let (mut doc, [a, _, c, _]) = stack();
        let folder = doc.add_folder(TimelineId::Main, &[a, c]).unwrap();
        assert_eq!(names(&doc), ["D", "Folder 1", "  C", "  A", "B"]);
        assert!(doc.layer(folder).unwrap().folder);
        assert_eq!(doc.folder_block(folder), [folder, c, a]);
    }

    #[test]
    fn restacking_a_folder_takes_its_contents_along() {
        let (mut doc, [a, b, _, _]) = stack();
        let folder = doc.add_folder(TimelineId::Main, &[a, b]).unwrap();
        // To the top of the stack, then tidied: its contents follow.
        doc.move_layer_to(folder, 4);
        doc.tidy_folders(TimelineId::Main);
        assert_eq!(names(&doc), ["Folder 1", "  B", "  A", "D", "C"]);
    }

    #[test]
    fn layers_move_into_and_out_of_folders() {
        let (mut doc, [a, b, c, d]) = stack();
        let folder = doc.add_folder(TimelineId::Main, &[b]).unwrap();
        doc.set_folder(d, Some(folder));
        assert_eq!(names(&doc), ["C", "Folder 1", "  D", "  B", "A"]);
        doc.set_folder(d, None);
        assert_eq!(names(&doc), ["C", "Folder 1", "  B", "D", "A"]);
        let _ = (a, c);
    }

    #[test]
    fn folders_nest_but_never_inside_themselves() {
        let (mut doc, [a, b, ..]) = stack();
        let inner = doc.add_folder(TimelineId::Main, &[a]).unwrap();
        let outer = doc.add_folder(TimelineId::Main, &[inner, b]).unwrap();
        assert!(doc.is_inside(a, outer));
        doc.set_folder(outer, Some(inner));
        assert_eq!(doc.layer(outer).unwrap().in_folder, None, "refused");
        assert_eq!(doc.folder_depth(doc.layer(a).unwrap()), 2);
    }

    #[test]
    fn deleting_a_folder_frees_its_contents() {
        let (mut doc, [a, b, ..]) = stack();
        let folder = doc.add_folder(TimelineId::Main, &[a, b]).unwrap();
        doc.remove_layer(folder);
        doc.tidy_folders(TimelineId::Main);
        assert_eq!(doc.layer(a).unwrap().in_folder, None);
        assert_eq!(names(&doc), ["D", "C", "B", "A"]);
    }

    #[test]
    fn dropping_a_layer_places_it_and_sets_its_folder() {
        let (mut doc, [a, b, c, d]) = stack();
        let folder = doc.add_folder(TimelineId::Main, &[b]).unwrap();
        // D dropped just under the folder's header: into it, above B.
        assert!(doc.place_layer(d, Some(b), Some(folder)));
        assert_eq!(names(&doc), ["C", "Folder 1", "  D", "  B", "A"]);
        // The folder dropped above A, out at the top level, takes its
        // contents along.
        assert!(doc.place_layer(folder, Some(a), None));
        assert_eq!(names(&doc), ["C", "Folder 1", "  D", "  B", "A"]);
        assert!(doc.place_layer(folder, None, None));
        assert_eq!(names(&doc), ["C", "A", "Folder 1", "  D", "  B"]);
        // Never into itself.
        assert!(!doc.place_layer(folder, Some(d), Some(folder)));
        let _ = c;
    }

    #[test]
    fn moving_up_and_down_steps_over_whole_folders() {
        let (mut doc, [a, b, c, d]) = stack();
        doc.add_folder(TimelineId::Main, &[b, c]).unwrap();
        assert_eq!(names(&doc), ["D", "Folder 1", "  C", "  B", "A"]);
        doc.move_among_siblings(a, 1);
        assert_eq!(names(&doc), ["D", "A", "Folder 1", "  C", "  B"]);
        doc.move_among_siblings(d, -1);
        assert_eq!(names(&doc), ["A", "D", "Folder 1", "  C", "  B"]);
        doc.move_among_siblings(d, -1);
        assert_eq!(names(&doc), ["A", "Folder 1", "  C", "  B", "D"]);
        // Inside a folder, only among its own contents.
        doc.move_among_siblings(b, 1);
        assert_eq!(names(&doc), ["A", "Folder 1", "  B", "  C", "D"]);
        doc.move_among_siblings(b, 1);
        assert_eq!(names(&doc), ["A", "Folder 1", "  B", "  C", "D"]);
    }

    #[test]
    fn pasted_layers_stay_in_a_folder_only_if_it_came_too() {
        let (mut doc, [a, b, ..]) = stack();
        let folder = doc.add_folder(TimelineId::Main, &[a, b]).unwrap();
        // The folder with its contents: a new folder holding new copies.
        let block = doc.copy_layers(&doc.folder_block(folder), None).unwrap();
        let pasted = doc.paste_layers(&block, TimelineId::Main, crate::Point::ZERO, None);
        assert_eq!(names(&doc)[..3], ["Folder 1 copy", "  B copy", "  A copy"]);
        assert!(doc.layer(pasted[1]).unwrap().in_folder.is_some_and(|f| f != folder));
        // A layer on its own: out of the folder it was in.
        let one = doc.copy_layers(&[a], None).unwrap();
        let pasted = doc.paste_layers(&one, TimelineId::Main, crate::Point::ZERO, None);
        assert_eq!(doc.layer(pasted[0]).unwrap().in_folder, None);
    }

    #[test]
    fn every_layer_has_a_colour_until_one_is_chosen() {
        let (mut doc, [a, b, ..]) = stack();
        assert_ne!(doc.layer(a).unwrap().tag_color(), doc.layer(b).unwrap().tag_color());
        doc.layer_mut(a).unwrap().color = Some(Color::BLACK);
        assert_eq!(doc.layer(a).unwrap().tag_color(), Color::BLACK);
    }

    #[test]
    fn folders_and_colours_survive_saving() {
        let (mut doc, [a, ..]) = stack();
        let folder = doc.add_folder(TimelineId::Main, &[a]).unwrap();
        doc.layer_mut(a).unwrap().color = Some(Color::rgb(1, 2, 3));
        let back = Document::from_json(&doc.to_json().unwrap()).unwrap();
        assert_eq!(back.layer(a).unwrap().in_folder, Some(folder));
        assert_eq!(back.layer(a).unwrap().color, Some(Color::rgb(1, 2, 3)));
        assert!(back.layer(folder).unwrap().folder);
    }
}
