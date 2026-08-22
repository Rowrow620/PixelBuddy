use crate::document::Document;

/// Default retained-memory ceiling for undo, redo, and suspended branch data.
pub const DEFAULT_HISTORY_BYTE_BUDGET: usize = 64 * 1024 * 1024;

pub trait Command {
    fn execute(&mut self, doc: &mut Document);
    fn undo(&mut self, doc: &mut Document);
    fn description(&self) -> &str;
    /// Conservative estimate of heap and inline bytes retained by this command.
    fn retained_bytes(&self) -> usize;
}

pub struct History {
    undo_stack: Vec<Box<dyn Command>>,
    redo_stack: Vec<Box<dyn Command>>,
    /// The redo path that was active when the user began a replacement edit.
    ///
    /// This is deliberately not exposed to the UI: the history panel should
    /// show only the path currently being edited. If every command on that
    /// replacement path is undone, this future is restored as the visible
    /// redo path and the replacement path is discarded.
    suspended_future: Option<SuspendedFuture>,
    max_size: usize,
    max_bytes: usize,
}

struct SuspendedFuture {
    /// Number of commands that were applied when the replacement path began.
    branch_point_undo_len: usize,
    /// Commands use the same ordering as `redo_stack`: the next command to
    /// redo is at the end of the vector.
    redo_stack: Vec<Box<dyn Command>>,
}

impl History {
    pub fn new(max_size: usize) -> Self {
        Self::with_limits(max_size, DEFAULT_HISTORY_BYTE_BUDGET)
    }

    pub fn with_limits(max_size: usize, max_bytes: usize) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            suspended_future: None,
            max_size,
            max_bytes,
        }
    }

    pub fn push(&mut self, mut command: Box<dyn Command>, doc: &mut Document) {
        command.execute(doc);
        self.push_applied(command);
    }

    /// Records a command whose changes have already been applied to `doc`.
    ///
    /// This is useful for snapshot-style structural edits. Re-executing a
    /// command after the caller has already changed the document can create
    /// transient invalid state or duplicate the change, so those edits should
    /// use this method instead of [`Self::push`].
    pub fn push_applied(&mut self, command: Box<dyn Command>) {
        self.prepare_for_new_command();
        self.undo_stack.push(command);
        self.trim_to_limits();
    }

    pub fn undo(&mut self, doc: &mut Document) {
        if let Some(mut command) = self.undo_stack.pop() {
            command.undo(doc);
            self.redo_stack.push(command);
            self.restore_suspended_future_at_branch_point();
        }
    }

    pub fn redo(&mut self, doc: &mut Document) {
        if let Some(mut command) = self.redo_stack.pop() {
            command.execute(doc);
            self.undo_stack.push(command);
        }
    }

    /// Discards undo/redo entries that no longer target the active document.
    ///
    /// Commands currently address layers by index, so retaining them across a
    /// frame switch or structural layer edit could modify a different target.
    /// Clearing is conservative, but prevents cross-frame data corruption until
    /// history uses stable frame and layer identifiers.
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.suspended_future = None;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn undo_descriptions(&self) -> Vec<String> {
        self.undo_stack
            .iter()
            .map(|cmd| cmd.description().to_string())
            .collect()
    }

    pub fn redo_descriptions(&self) -> Vec<String> {
        self.redo_stack
            .iter()
            .map(|cmd| cmd.description().to_string())
            .collect()
    }

    pub fn entry_count(&self) -> usize {
        self.undo_stack.len()
            + self.redo_stack.len()
            + self
                .suspended_future
                .as_ref()
                .map_or(0, |future| future.redo_stack.len())
    }

    pub fn retained_bytes(&self) -> usize {
        let stack_bytes = |stack: &[Box<dyn Command>]| {
            stack.iter().fold(0usize, |total, command| {
                total.saturating_add(command.retained_bytes())
            })
        };
        stack_bytes(&self.undo_stack)
            .saturating_add(stack_bytes(&self.redo_stack))
            .saturating_add(
                self.suspended_future
                    .as_ref()
                    .map_or(0, |future| stack_bytes(&future.redo_stack)),
            )
    }

    pub fn jump_to_undo_index(&mut self, target_idx: usize, doc: &mut Document) {
        if target_idx < self.undo_stack.len() {
            let undo_count = self.undo_stack.len() - (target_idx + 1);
            for _ in 0..undo_count {
                self.undo(doc);
            }
        }
    }

    /// Starts a replacement path after the user has undone one or more
    /// commands. The prior redo stack is kept privately so undoing the entire
    /// replacement path can put the user back on the prior linear history.
    fn prepare_for_new_command(&mut self) {
        if self.suspended_future.is_none() && !self.redo_stack.is_empty() {
            self.suspended_future = Some(SuspendedFuture {
                branch_point_undo_len: self.undo_stack.len(),
                redo_stack: std::mem::take(&mut self.redo_stack),
            });
        } else {
            // Any redo entries here belong to the current replacement path,
            // so a new command replaces that visible future as normal.
            self.redo_stack.clear();
        }
    }

    /// Restores the hidden pre-branch future after the replacement path has
    /// been fully undone. The commands just undone form the discarded branch,
    /// so they are intentionally removed from the visible redo stack.
    fn restore_suspended_future_at_branch_point(&mut self) {
        let at_branch_point = self
            .suspended_future
            .as_ref()
            .is_some_and(|future| self.undo_stack.len() == future.branch_point_undo_len);

        if at_branch_point {
            let future = self
                .suspended_future
                .take()
                .expect("the suspended future was checked above");
            self.redo_stack = future.redo_stack;
        }
    }

    /// Enforces count and retained-byte limits across every live history path.
    /// Oldest applied transactions are discarded first. If only a redo or
    /// suspended future remains, its farthest future transaction is discarded.
    fn trim_to_limits(&mut self) {
        while self.entry_count() > self.max_size || self.retained_bytes() > self.max_bytes {
            if !self.undo_stack.is_empty() {
                self.undo_stack.remove(0);
                let discard_suspended_future =
                    self.suspended_future.as_mut().is_some_and(|future| {
                        if future.branch_point_undo_len == 0 {
                            true
                        } else {
                            future.branch_point_undo_len -= 1;
                            false
                        }
                    });
                if discard_suspended_future {
                    self.suspended_future = None;
                }
                continue;
            }

            if let Some(future) = self.suspended_future.as_mut() {
                if !future.redo_stack.is_empty() {
                    future.redo_stack.remove(0);
                    if future.redo_stack.is_empty() {
                        self.suspended_future = None;
                    }
                    continue;
                }
                self.suspended_future = None;
                continue;
            }

            if !self.redo_stack.is_empty() {
                self.redo_stack.remove(0);
                continue;
            }
            break;
        }
    }
}

pub struct DrawCommand {
    layer_index: usize,
    /// (x, y, old_color, new_color)
    changes: Vec<(u32, u32, [u8; 4], [u8; 4])>,
}

/// Stores a complete before/after document pair for an already-applied
/// structural edit.
///
/// Pixel commands can safely target a layer by index because they only live
/// until a structural change clears history. Layer and palette operations
/// themselves need a different representation: restoring a complete snapshot
/// avoids relying on stale indices while stable IDs are not available yet.
pub struct DocumentSnapshotCommand {
    description: String,
    before: Document,
    after: Document,
}

impl DocumentSnapshotCommand {
    pub fn new(description: impl Into<String>, before: Document, after: Document) -> Self {
        Self {
            description: description.into(),
            before,
            after,
        }
    }
}

impl Command for DocumentSnapshotCommand {
    fn execute(&mut self, doc: &mut Document) {
        *doc = self.after.clone();
    }

    fn undo(&mut self, doc: &mut Document) {
        *doc = self.before.clone();
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            .saturating_add(self.description.capacity())
            .saturating_add(document_retained_bytes(&self.before))
            .saturating_add(document_retained_bytes(&self.after))
    }
}

impl DrawCommand {
    pub fn new(layer_index: usize, changes: Vec<(u32, u32, [u8; 4], [u8; 4])>) -> Self {
        Self {
            layer_index,
            changes,
        }
    }
}

impl Command for DrawCommand {
    fn execute(&mut self, doc: &mut Document) {
        if self.layer_index < doc.layers.len() {
            let canvas = &mut doc.layers[self.layer_index].canvas;
            for &(x, y, _, new_color) in &self.changes {
                canvas.set_pixel(x, y, new_color);
            }
        }
    }

    fn undo(&mut self, doc: &mut Document) {
        if self.layer_index < doc.layers.len() {
            let canvas = &mut doc.layers[self.layer_index].canvas;
            for &(x, y, old_color, _) in self.changes.iter().rev() {
                canvas.set_pixel(x, y, old_color);
            }
        }
    }

    fn description(&self) -> &str {
        "Draw pixels"
    }

    fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>().saturating_add(
            self.changes
                .capacity()
                .saturating_mul(std::mem::size_of::<(u32, u32, [u8; 4], [u8; 4])>()),
        )
    }
}

fn document_retained_bytes(document: &Document) -> usize {
    let layer_bytes = document.layers.iter().fold(0usize, |total, layer| {
        total
            .saturating_add(layer.name.capacity())
            .saturating_add(layer.canvas.retained_pixel_bytes())
    });
    std::mem::size_of::<Document>()
        .saturating_add(
            document
                .layers
                .capacity()
                .saturating_mul(std::mem::size_of::<crate::document::Layer>()),
        )
        .saturating_add(layer_bytes)
        .saturating_add(
            document
                .palette
                .colors
                .capacity()
                .saturating_mul(std::mem::size_of::<[u8; 4]>()),
        )
}

#[cfg(test)]
mod tests {
    use super::{Command, DocumentSnapshotCommand, DrawCommand, History};
    use crate::document::Document;

    struct PixelCommand {
        description: &'static str,
        old_color: [u8; 4],
        new_color: [u8; 4],
    }

    impl PixelCommand {
        fn new(description: &'static str, old_color: [u8; 4], new_color: [u8; 4]) -> Self {
            Self {
                description,
                old_color,
                new_color,
            }
        }
    }

    impl Command for PixelCommand {
        fn execute(&mut self, doc: &mut Document) {
            doc.active_layer_mut()
                .canvas
                .set_pixel(0, 0, self.new_color);
        }

        fn undo(&mut self, doc: &mut Document) {
            doc.active_layer_mut()
                .canvas
                .set_pixel(0, 0, self.old_color);
        }

        fn description(&self) -> &str {
            self.description
        }

        fn retained_bytes(&self) -> usize {
            std::mem::size_of::<Self>()
        }
    }

    fn color(value: u8) -> [u8; 4] {
        [value, 0, 0, 255]
    }

    fn push_pixel_command(
        history: &mut History,
        document: &mut Document,
        description: &'static str,
        old_color: [u8; 4],
        new_color: [u8; 4],
    ) {
        history.push(
            Box::new(PixelCommand::new(description, old_color, new_color)),
            document,
        );
    }

    #[test]
    fn snapshot_command_restores_structural_document_changes() {
        let mut document = Document::new(2, 2);
        let before = document.clone();
        document.add_layer();
        document.active_layer_mut().name = "Ink".to_owned();
        let after = document.clone();

        let mut history = History::new(10);
        history.push_applied(Box::new(DocumentSnapshotCommand::new(
            "Add layer",
            before,
            after,
        )));

        assert_eq!(document.layers.len(), 2);
        assert_eq!(document.active_layer().name, "Ink");
        history.undo(&mut document);
        assert_eq!(document.layers.len(), 1);
        assert_eq!(document.active_layer().name, "Layer 1");
        history.redo(&mut document);
        assert_eq!(document.layers.len(), 2);
        assert_eq!(document.active_layer().name, "Ink");
    }

    #[test]
    fn fully_undoing_a_replacement_path_restores_the_prior_redo_path() {
        let mut document = Document::new(1, 1);
        let mut history = History::new(10);
        let transparent = [0, 0, 0, 0];

        push_pixel_command(&mut history, &mut document, "Step 1", transparent, color(1));
        push_pixel_command(&mut history, &mut document, "Step 2", color(1), color(2));
        push_pixel_command(&mut history, &mut document, "Step 3", color(2), color(3));
        push_pixel_command(&mut history, &mut document, "Step 4", color(3), color(4));
        push_pixel_command(&mut history, &mut document, "Step 5", color(4), color(5));

        history.jump_to_undo_index(0, &mut document);
        assert_eq!(document.active_layer().canvas.get_pixel(0, 0), color(1));
        assert_eq!(history.undo_descriptions(), vec!["Step 1"]);
        assert_eq!(
            history.redo_descriptions(),
            vec!["Step 5", "Step 4", "Step 3", "Step 2"]
        );

        push_pixel_command(
            &mut history,
            &mut document,
            "Replacement step 2",
            color(1),
            color(9),
        );
        assert_eq!(
            history.undo_descriptions(),
            vec!["Step 1", "Replacement step 2"]
        );
        assert!(history.redo_descriptions().is_empty());

        history.undo(&mut document);

        assert_eq!(document.active_layer().canvas.get_pixel(0, 0), color(1));
        assert_eq!(history.undo_descriptions(), vec!["Step 1"]);
        assert_eq!(
            history.redo_descriptions(),
            vec!["Step 5", "Step 4", "Step 3", "Step 2"]
        );

        history.redo(&mut document);
        assert_eq!(document.active_layer().canvas.get_pixel(0, 0), color(2));
        assert_eq!(history.undo_descriptions(), vec!["Step 1", "Step 2"]);
    }

    #[test]
    fn suspended_future_stays_hidden_until_every_replacement_edit_is_undone() {
        let mut document = Document::new(1, 1);
        let mut history = History::new(10);
        let transparent = [0, 0, 0, 0];

        push_pixel_command(&mut history, &mut document, "Step 1", transparent, color(1));
        push_pixel_command(&mut history, &mut document, "Step 2", color(1), color(2));
        push_pixel_command(&mut history, &mut document, "Step 3", color(2), color(3));
        history.jump_to_undo_index(0, &mut document);

        push_pixel_command(
            &mut history,
            &mut document,
            "Replacement step 2",
            color(1),
            color(9),
        );
        push_pixel_command(
            &mut history,
            &mut document,
            "Replacement step 3",
            color(9),
            color(10),
        );

        history.undo(&mut document);
        assert_eq!(document.active_layer().canvas.get_pixel(0, 0), color(9));
        assert_eq!(
            history.undo_descriptions(),
            vec!["Step 1", "Replacement step 2"]
        );
        assert_eq!(history.redo_descriptions(), vec!["Replacement step 3"]);

        history.undo(&mut document);
        assert_eq!(document.active_layer().canvas.get_pixel(0, 0), color(1));
        assert_eq!(history.undo_descriptions(), vec!["Step 1"]);
        assert_eq!(history.redo_descriptions(), vec!["Step 3", "Step 2"]);
    }

    #[test]
    fn evicting_a_replacement_command_discards_an_unreachable_suspended_future() {
        let mut document = Document::new(1, 1);
        let mut history = History::new(2);
        let transparent = [0, 0, 0, 0];

        push_pixel_command(&mut history, &mut document, "Step 1", transparent, color(1));
        push_pixel_command(&mut history, &mut document, "Step 2", color(1), color(2));
        history.undo(&mut document);

        push_pixel_command(
            &mut history,
            &mut document,
            "Replacement step 2",
            color(1),
            color(9),
        );
        push_pixel_command(
            &mut history,
            &mut document,
            "Replacement step 3",
            color(9),
            color(10),
        );
        push_pixel_command(
            &mut history,
            &mut document,
            "Replacement step 4",
            color(10),
            color(11),
        );

        history.undo(&mut document);
        history.undo(&mut document);

        assert_eq!(document.active_layer().canvas.get_pixel(0, 0), color(9));
        assert_eq!(history.undo_descriptions(), Vec::<String>::new());
        assert_eq!(
            history.redo_descriptions(),
            vec!["Replacement step 4", "Replacement step 3"]
        );
    }

    #[test]
    fn byte_budget_evicts_oldest_transactions_even_below_the_count_limit() {
        let mut document = Document::new(1, 1);
        let command_bytes = std::mem::size_of::<PixelCommand>();
        let mut history = History::with_limits(100, command_bytes * 2);
        let transparent = [0, 0, 0, 0];

        push_pixel_command(&mut history, &mut document, "Step 1", transparent, color(1));
        push_pixel_command(&mut history, &mut document, "Step 2", color(1), color(2));
        push_pixel_command(&mut history, &mut document, "Step 3", color(2), color(3));

        assert_eq!(history.undo_descriptions(), vec!["Step 2", "Step 3"]);
        assert_eq!(history.entry_count(), 2);
        assert!(history.retained_bytes() <= command_bytes * 2);
        history.undo(&mut document);
        history.undo(&mut document);
        assert_eq!(document.active_layer().canvas.get_pixel(0, 0), color(1));
    }

    #[test]
    fn repeated_large_fills_remain_within_the_history_byte_budget() {
        let mut document = Document::new(16, 16);
        let first_changes: Vec<_> = (0..16)
            .flat_map(|y| (0..16).map(move |x| (x, y, [0, 0, 0, 0], [255, 0, 0, 255])))
            .collect();
        let second_changes: Vec<_> = (0..16)
            .flat_map(|y| (0..16).map(move |x| (x, y, [255, 0, 0, 255], [0, 0, 255, 255])))
            .collect();
        let command_bytes = DrawCommand::new(0, first_changes.clone()).retained_bytes();
        let mut history = History::with_limits(100, command_bytes);

        history.push(Box::new(DrawCommand::new(0, first_changes)), &mut document);
        history.push(Box::new(DrawCommand::new(0, second_changes)), &mut document);

        assert_eq!(history.entry_count(), 1);
        assert!(history.retained_bytes() <= command_bytes);
        history.undo(&mut document);
        assert_eq!(
            document.active_layer().canvas.get_pixel(8, 8),
            [255, 0, 0, 255]
        );
    }
    #[test]
    fn an_oversized_snapshot_is_applied_but_not_retained_for_undo() {
        let before = Document::new(64, 64);
        let mut after = before.clone();
        after
            .active_layer_mut()
            .canvas
            .set_pixel(0, 0, [9, 8, 7, 255]);
        let command = DocumentSnapshotCommand::new("Large structural edit", before, after);
        let command_bytes = command.retained_bytes();
        let mut history = History::with_limits(100, command_bytes.saturating_sub(1));

        history.push_applied(Box::new(command));

        assert!(!history.can_undo());
        assert_eq!(history.entry_count(), 0);
        assert_eq!(history.retained_bytes(), 0);
    }
    #[test]
    fn clearing_history_discards_a_suspended_future() {
        let mut document = Document::new(1, 1);
        let mut history = History::new(10);
        let transparent = [0, 0, 0, 0];

        push_pixel_command(
            &mut history,
            &mut document,
            "Original step",
            transparent,
            color(1),
        );
        history.undo(&mut document);
        push_pixel_command(
            &mut history,
            &mut document,
            "Replacement step",
            transparent,
            color(9),
        );

        history.clear();
        push_pixel_command(
            &mut history,
            &mut document,
            "Post-clear step",
            color(9),
            color(10),
        );
        history.undo(&mut document);

        assert_eq!(document.active_layer().canvas.get_pixel(0, 0), color(9));
        assert_eq!(history.redo_descriptions(), vec!["Post-clear step"]);
    }
}
