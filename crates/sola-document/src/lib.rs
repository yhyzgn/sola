use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockKind {
    Heading { level: u8 },
    Paragraph,
    ListItem { ordered: bool },
    Quote,
    CodeFence { language: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentBlock {
    pub id: usize,
    pub kind: BlockKind,
    pub source: String,
    pub rendered: String,
    pub draft: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineEntry {
    pub level: u8,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DocumentStats {
    pub headings: usize,
    pub paragraphs: usize,
    pub code_blocks: usize,
    pub list_items: usize,
    pub quotes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DocumentSnapshot {
    source: String,
    blocks: Vec<DocumentBlock>,
    outline: Vec<OutlineEntry>,
    stats: DocumentStats,
    focused_block: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentModel {
    source: String,
    blocks: Vec<DocumentBlock>,
    outline: Vec<OutlineEntry>,
    stats: DocumentStats,
    focused_block: usize,
    undo_stack: Vec<DocumentSnapshot>,
    redo_stack: Vec<DocumentSnapshot>,
}

impl DocumentModel {
    pub fn from_markdown(source: impl Into<String>) -> Self {
        let source = source.into();
        let blocks = parse_blocks(&source);
        let outline = build_outline(&source);
        let stats = build_stats(&blocks);

        Self {
            source,
            blocks,
            outline,
            stats,
            focused_block: 0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn blocks(&self) -> &[DocumentBlock] {
        &self.blocks
    }

    pub fn outline(&self) -> &[OutlineEntry] {
        &self.outline
    }

    pub fn stats(&self) -> &DocumentStats {
        &self.stats
    }

    pub fn focused_block(&self) -> usize {
        self.focused_block
    }

    pub fn focused_block_ref(&self) -> Option<&DocumentBlock> {
        self.blocks.get(self.focused_block)
    }

    pub fn focused_block_mut(&mut self) -> Option<&mut DocumentBlock> {
        self.blocks.get_mut(self.focused_block)
    }

    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn undo(&mut self) -> bool {
        let Some(snapshot) = self.undo_stack.pop() else {
            return false;
        };

        self.redo_stack.push(self.snapshot());
        self.restore_snapshot(snapshot);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(snapshot) = self.redo_stack.pop() else {
            return false;
        };

        self.undo_stack.push(self.snapshot());
        self.restore_snapshot(snapshot);
        true
    }

    pub fn focus_block(&mut self, index: usize) -> bool {
        if index >= self.blocks.len() {
            return false;
        }

        self.focused_block = index;
        true
    }

    pub fn focus_next(&mut self) -> bool {
        if self.blocks.is_empty() || self.focused_block + 1 >= self.blocks.len() {
            return false;
        }

        self.focused_block += 1;
        true
    }

    pub fn focus_previous(&mut self) -> bool {
        if self.blocks.is_empty() || self.focused_block == 0 {
            return false;
        }

        self.focused_block -= 1;
        true
    }

    pub fn focused_text(&self) -> Option<&str> {
        let block = self.focused_block_ref()?;
        Some(block.draft.as_deref().unwrap_or(&block.source))
    }

    pub fn focused_has_draft(&self) -> bool {
        self.focused_block_ref()
            .and_then(|block| block.draft.as_ref())
            .is_some()
    }

    pub fn set_focused_draft(&mut self, draft: String) -> bool {
        let Some(block) = self.focused_block_ref() else {
            return false;
        };

        let next_draft = if draft == block.source {
            None
        } else {
            Some(draft)
        };

        if block.draft == next_draft {
            return false;
        }

        self.push_undo_snapshot();
        self.blocks[self.focused_block].draft = next_draft;
        true
    }

    pub fn append_to_focused_draft(&mut self, suffix: &str) -> bool {
        let Some(current) = self.focused_text().map(ToOwned::to_owned) else {
            return false;
        };

        let next = format!("{current}{suffix}");
        self.set_focused_draft(next)
    }

    pub fn push_char_to_focused_draft(&mut self, ch: char) -> bool {
        let Some(current) = self.focused_text().map(ToOwned::to_owned) else {
            return false;
        };

        let next = format!("{current}{ch}");
        self.set_focused_draft(next)
    }

    pub fn delete_last_char_from_focused_draft(&mut self) -> bool {
        let Some(current) = self.focused_text().map(ToOwned::to_owned) else {
            return false;
        };

        let Some((next, _)) = current.char_indices().next_back() else {
            return false;
        };

        self.set_focused_draft(current[..next].to_string())
    }

    pub fn revert_focused_draft(&mut self) -> bool {
        let Some(block) = self.focused_block_ref() else {
            return false;
        };

        if block.draft.is_none() {
            return false;
        }

        self.push_undo_snapshot();
        self.blocks[self.focused_block].draft = None;
        true
    }

    pub fn apply_focused_draft(&mut self) -> bool {
        let index = self.focused_block;
        let Some(block) = self.blocks.get(index) else {
            return false;
        };

        let Some(draft) = block.draft.clone() else {
            return false;
        };

        self.push_undo_snapshot();

        let block = &mut self.blocks[index];
        block.draft = None;
        block.source = draft;
        block.rendered = render_block_source(&block.kind, &block.source);

        self.rebuild_metadata();
        true
    }

    pub fn insert_paragraph_after_focused(&mut self, text: impl Into<String>) -> bool {
        let text = text.into().trim().to_string();
        if text.is_empty() {
            return false;
        }

        let insert_at = if self.blocks.is_empty() {
            0
        } else {
            self.focused_block + 1
        };

        self.push_undo_snapshot();
        self.blocks.insert(
            insert_at,
            DocumentBlock {
                id: insert_at,
                kind: BlockKind::Paragraph,
                source: text.clone(),
                rendered: text,
                draft: None,
            },
        );
        self.focused_block = insert_at;
        self.rebuild_metadata();
        true
    }

    pub fn duplicate_focused_block(&mut self) -> bool {
        let Some(block) = self.focused_block_ref().cloned() else {
            return false;
        };

        let insert_at = self.focused_block + 1;
        self.push_undo_snapshot();
        self.blocks.insert(
            insert_at,
            DocumentBlock {
                id: insert_at,
                kind: block.kind,
                source: block.source,
                rendered: block.rendered,
                draft: None,
            },
        );
        self.focused_block = insert_at;
        self.rebuild_metadata();
        true
    }

    pub fn delete_focused_block(&mut self) -> bool {
        if self.blocks.len() <= 1 || self.focused_block >= self.blocks.len() {
            return false;
        }

        self.push_undo_snapshot();
        self.blocks.remove(self.focused_block);
        if self.focused_block >= self.blocks.len() {
            self.focused_block = self.blocks.len().saturating_sub(1);
        }
        self.rebuild_metadata();
        true
    }
}

impl BlockKind {
    pub fn label(&self) -> &'static str {
        match self {
            BlockKind::Heading { .. } => "heading",
            BlockKind::Paragraph => "paragraph",
            BlockKind::ListItem { .. } => "list",
            BlockKind::Quote => "quote",
            BlockKind::CodeFence { .. } => "code",
        }
    }
}

fn parse_blocks(source: &str) -> Vec<DocumentBlock> {
    let mut blocks = Vec::new();
    let mut paragraph_lines: Vec<String> = Vec::new();
    let mut code_lines: Vec<String> = Vec::new();
    let mut code_language: Option<String> = None;

    let flush_paragraph = |blocks: &mut Vec<DocumentBlock>, paragraph_lines: &mut Vec<String>| {
        if paragraph_lines.is_empty() {
            return;
        }

        let content = paragraph_lines.join("\n").trim().to_string();
        if content.is_empty() {
            paragraph_lines.clear();
            return;
        }

        let id = blocks.len();
        blocks.push(DocumentBlock {
            id,
            kind: BlockKind::Paragraph,
            rendered: content.clone(),
            source: content,
            draft: None,
        });
        paragraph_lines.clear();
    };

    for raw_line in source.lines() {
        let line = raw_line.trim_end();

        if code_language.is_some() {
            if line.trim_start().starts_with("```") {
                let id = blocks.len();
                let code = code_lines.join("\n");
                blocks.push(DocumentBlock {
                    id,
                    kind: BlockKind::CodeFence {
                        language: code_language.take(),
                    },
                    rendered: code.clone(),
                    source: format!("```\n{}\n```", code),
                    draft: None,
                });
                code_lines.clear();
                continue;
            }

            code_lines.push(line.to_string());
            continue;
        }

        if line.trim().is_empty() {
            flush_paragraph(&mut blocks, &mut paragraph_lines);
            continue;
        }

        if let Some(info) = line.trim_start().strip_prefix("```") {
            flush_paragraph(&mut blocks, &mut paragraph_lines);
            let info = info.trim();
            code_language = (!info.is_empty()).then(|| info.to_string());
            code_lines.clear();
            continue;
        }

        if let Some((level, text)) = parse_heading(line) {
            flush_paragraph(&mut blocks, &mut paragraph_lines);
            let id = blocks.len();
            blocks.push(DocumentBlock {
                id,
                kind: BlockKind::Heading { level },
                rendered: text.clone(),
                source: line.to_string(),
                draft: None,
            });
            continue;
        }

        if let Some(text) = line.trim_start().strip_prefix("> ") {
            flush_paragraph(&mut blocks, &mut paragraph_lines);
            let id = blocks.len();
            blocks.push(DocumentBlock {
                id,
                kind: BlockKind::Quote,
                rendered: text.to_string(),
                source: line.to_string(),
                draft: None,
            });
            continue;
        }

        if let Some(text) = parse_unordered_list_item(line) {
            flush_paragraph(&mut blocks, &mut paragraph_lines);
            let id = blocks.len();
            blocks.push(DocumentBlock {
                id,
                kind: BlockKind::ListItem { ordered: false },
                rendered: text.to_string(),
                source: line.to_string(),
                draft: None,
            });
            continue;
        }

        if let Some(text) = parse_ordered_list_item(line) {
            flush_paragraph(&mut blocks, &mut paragraph_lines);
            let id = blocks.len();
            blocks.push(DocumentBlock {
                id,
                kind: BlockKind::ListItem { ordered: true },
                rendered: text.to_string(),
                source: line.to_string(),
                draft: None,
            });
            continue;
        }

        paragraph_lines.push(line.to_string());
    }

    if code_language.is_some() {
        let id = blocks.len();
        let code = code_lines.join("\n");
        blocks.push(DocumentBlock {
            id,
            kind: BlockKind::CodeFence {
                language: code_language.take(),
            },
            rendered: code.clone(),
            source: format!("```\n{}\n```", code),
            draft: None,
        });
    }

    flush_paragraph(&mut blocks, &mut paragraph_lines);

    blocks
}

fn build_outline(source: &str) -> Vec<OutlineEntry> {
    let mut outline = Vec::new();
    let mut current_heading_level: Option<u8> = None;
    let mut current_heading = String::new();

    for event in Parser::new_ext(source, Options::all()) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                current_heading_level = Some(heading_level_to_u8(level));
                current_heading.clear();
            }
            Event::Text(text) | Event::Code(text) => {
                if current_heading_level.is_some() {
                    current_heading.push_str(&text);
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some(level) = current_heading_level.take() {
                    outline.push(OutlineEntry {
                        level,
                        title: current_heading.trim().to_string(),
                    });
                }
            }
            _ => {}
        }
    }

    outline
}

fn build_stats(blocks: &[DocumentBlock]) -> DocumentStats {
    let mut stats = DocumentStats::default();

    for block in blocks {
        match block.kind {
            BlockKind::Heading { .. } => stats.headings += 1,
            BlockKind::Paragraph => stats.paragraphs += 1,
            BlockKind::ListItem { .. } => stats.list_items += 1,
            BlockKind::Quote => stats.quotes += 1,
            BlockKind::CodeFence { .. } => stats.code_blocks += 1,
        }
    }

    stats
}

fn serialize_blocks(blocks: &[DocumentBlock]) -> String {
    blocks
        .iter()
        .map(|block| block.source.as_str())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn render_block_source(kind: &BlockKind, source: &str) -> String {
    match kind {
        BlockKind::Heading { .. } => parse_heading(source)
            .map(|(_, text)| text)
            .unwrap_or_else(|| source.trim().to_string()),
        BlockKind::Paragraph => source.trim().to_string(),
        BlockKind::ListItem { ordered } => {
            if *ordered {
                parse_ordered_list_item(source)
                    .unwrap_or(source.trim())
                    .to_string()
            } else {
                parse_unordered_list_item(source)
                    .unwrap_or(source.trim())
                    .to_string()
            }
        }
        BlockKind::Quote => source
            .trim_start()
            .strip_prefix("> ")
            .unwrap_or(source.trim())
            .to_string(),
        BlockKind::CodeFence { .. } => source
            .lines()
            .skip(1)
            .take_while(|line| !line.trim_start().starts_with("```"))
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn parse_heading(line: &str) -> Option<(u8, String)> {
    let trimmed = line.trim_start();
    let hashes = trimmed.chars().take_while(|ch| *ch == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }

    let text = trimmed[hashes..].trim();
    if text.is_empty() {
        return None;
    }

    Some((hashes as u8, text.to_string()))
}

fn parse_unordered_list_item(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
}

fn parse_ordered_list_item(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let dot_position = trimmed.find(". ")?;
    let (digits, rest) = trimmed.split_at(dot_position);
    if digits.is_empty() || !digits.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }

    rest.strip_prefix(". ")
}

fn heading_level_to_u8(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

impl DocumentModel {
    fn snapshot(&self) -> DocumentSnapshot {
        DocumentSnapshot {
            source: self.source.clone(),
            blocks: self.blocks.clone(),
            outline: self.outline.clone(),
            stats: self.stats.clone(),
            focused_block: self.focused_block,
        }
    }

    fn restore_snapshot(&mut self, snapshot: DocumentSnapshot) {
        self.source = snapshot.source;
        self.blocks = snapshot.blocks;
        self.outline = snapshot.outline;
        self.stats = snapshot.stats;
        self.focused_block = snapshot.focused_block;
    }

    fn push_undo_snapshot(&mut self) {
        self.undo_stack.push(self.snapshot());
        self.redo_stack.clear();
    }

    fn rebuild_metadata(&mut self) {
        for (index, block) in self.blocks.iter_mut().enumerate() {
            block.id = index;
        }
        self.source = serialize_blocks(&self.blocks);
        self.outline = build_outline(&self.source);
        self.stats = build_stats(&self.blocks);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_parser_produces_blocks_for_common_types() {
        let document = DocumentModel::from_markdown(
            r#"# Title

Paragraph

- item

> quote

```rust
fn main() {}
```
"#,
        );

        assert!(!document.blocks().is_empty());
        assert!(matches!(
            document.blocks()[0].kind,
            BlockKind::Heading { level: 1 }
        ));
        assert!(
            document
                .blocks()
                .iter()
                .any(|block| matches!(block.kind, BlockKind::CodeFence { .. }))
        );
    }

    #[test]
    fn outline_comes_from_pulldown_cmark() {
        let document = DocumentModel::from_markdown("# A\n\n## B\n\nText");

        assert_eq!(document.outline().len(), 2);
        assert_eq!(document.outline()[0].title, "A");
        assert_eq!(document.outline()[1].level, 2);
    }

    #[test]
    fn focus_block_bounds_are_checked() {
        let mut document = DocumentModel::from_markdown("# A\n\nText");

        assert!(document.focus_block(1));
        assert_eq!(document.focused_block(), 1);
        assert!(!document.focus_block(99));
        assert_eq!(document.focused_block(), 1);
    }

    #[test]
    fn focus_navigation_moves_between_blocks() {
        let mut document = DocumentModel::from_markdown("# A\n\nText\n\n- item");

        assert_eq!(document.block_count(), 3);
        assert!(document.focus_next());
        assert_eq!(document.focused_block(), 1);
        assert!(document.focus_next());
        assert_eq!(document.focused_block(), 2);
        assert!(!document.focus_next());
        assert!(document.focus_previous());
        assert_eq!(document.focused_block(), 1);
    }

    #[test]
    fn focused_draft_can_be_applied() {
        let mut document = DocumentModel::from_markdown("# Title\n\nParagraph");

        assert!(document.focus_block(1));
        assert!(document.set_focused_draft("Paragraph edited".to_string()));
        assert!(document.focused_has_draft());
        assert_eq!(document.focused_text(), Some("Paragraph edited"));
        assert!(document.apply_focused_draft());
        assert!(!document.focused_has_draft());
        assert_eq!(document.blocks()[1].rendered, "Paragraph edited");
        assert!(document.source().contains("Paragraph edited"));
    }

    #[test]
    fn focused_draft_can_be_reverted() {
        let mut document = DocumentModel::from_markdown("# Title\n\nParagraph");

        assert!(document.focus_block(1));
        assert!(document.append_to_focused_draft("\nprototype edit"));
        assert!(document.focused_has_draft());
        assert!(document.revert_focused_draft());
        assert!(!document.focused_has_draft());
        assert_eq!(document.focused_text(), Some("Paragraph"));
    }

    #[test]
    fn focused_draft_can_be_typed_and_backspaced() {
        let mut document = DocumentModel::from_markdown("# Title\n\nParagraph");

        assert!(document.focus_block(1));
        assert!(document.push_char_to_focused_draft('!'));
        assert_eq!(document.focused_text(), Some("Paragraph!"));
        assert!(document.delete_last_char_from_focused_draft());
        assert_eq!(document.focused_text(), Some("Paragraph"));
    }

    #[test]
    fn paragraph_can_be_inserted_after_focus() {
        let mut document = DocumentModel::from_markdown("# Title\n\nParagraph");

        assert!(document.insert_paragraph_after_focused("Inserted paragraph"));
        assert_eq!(document.block_count(), 3);
        assert_eq!(document.focused_block(), 1);
        assert_eq!(document.blocks()[1].rendered, "Inserted paragraph");
    }

    #[test]
    fn focused_block_can_be_duplicated() {
        let mut document = DocumentModel::from_markdown("# Title\n\nParagraph");

        assert!(document.focus_block(1));
        assert!(document.duplicate_focused_block());
        assert_eq!(document.block_count(), 3);
        assert_eq!(document.focused_block(), 2);
        assert_eq!(document.blocks()[2].rendered, "Paragraph");
    }

    #[test]
    fn focused_block_can_be_deleted_when_more_than_one_block_exists() {
        let mut document = DocumentModel::from_markdown("# Title\n\nParagraph");

        assert!(document.focus_block(1));
        assert!(document.delete_focused_block());
        assert_eq!(document.block_count(), 1);
        assert_eq!(document.focused_block(), 0);
        assert_eq!(document.blocks()[0].rendered, "Title");
    }

    #[test]
    fn undo_and_redo_restore_focused_draft_edits() {
        let mut document = DocumentModel::from_markdown("# Title\n\nParagraph");

        assert!(document.focus_block(1));
        assert!(document.push_char_to_focused_draft('!'));
        assert_eq!(document.focused_text(), Some("Paragraph!"));
        assert!(document.can_undo());

        assert!(document.undo());
        assert_eq!(document.focused_text(), Some("Paragraph"));
        assert!(!document.focused_has_draft());
        assert!(document.can_redo());

        assert!(document.redo());
        assert_eq!(document.focused_text(), Some("Paragraph!"));
        assert!(document.focused_has_draft());
    }

    #[test]
    fn undo_and_redo_restore_structural_edits() {
        let mut document = DocumentModel::from_markdown("# Title\n\nParagraph");

        assert!(document.focus_block(1));
        assert!(document.duplicate_focused_block());
        assert_eq!(document.block_count(), 3);
        assert_eq!(document.focused_block(), 2);

        assert!(document.undo());
        assert_eq!(document.block_count(), 2);
        assert_eq!(document.focused_block(), 1);
        assert_eq!(document.blocks()[1].rendered, "Paragraph");

        assert!(document.redo());
        assert_eq!(document.block_count(), 3);
        assert_eq!(document.focused_block(), 2);
        assert_eq!(document.blocks()[2].rendered, "Paragraph");
    }

    #[test]
    fn new_edit_clears_redo_history() {
        let mut document = DocumentModel::from_markdown("# Title\n\nParagraph");

        assert!(document.focus_block(1));
        assert!(document.push_char_to_focused_draft('!'));
        assert!(document.undo());
        assert!(document.can_redo());

        assert!(document.push_char_to_focused_draft('?'));
        assert!(!document.can_redo());
        assert_eq!(document.focused_text(), Some("Paragraph?"));
    }
}
