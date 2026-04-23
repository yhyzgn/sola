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
pub struct DocumentModel {
    source: String,
    blocks: Vec<DocumentBlock>,
    outline: Vec<OutlineEntry>,
    stats: DocumentStats,
    focused_block: usize,
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

    pub fn block_count(&self) -> usize {
        self.blocks.len()
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
}
