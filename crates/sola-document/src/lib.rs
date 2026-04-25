use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

pub mod highlighter;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockKind {
    Heading { level: u8 },
    Paragraph,
    ListItem { ordered: bool },
    Quote,
    CodeFence { language: Option<String> },
    MathBlock,
    TypstBlock,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HtmlNode {
    Text(String),
    StyledText(HtmlStyledText),
    Image(HtmlImage),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlStyledText {
    pub text: String,
    pub color: Option<String>,
    pub font_size_px: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlImage {
    pub src: Option<String>,
    pub alt: Option<String>,
    pub width_px: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HtmlAdapter {
    Adapted { nodes: Vec<HtmlNode> },
    Unsupported { raw: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypstAdapter {
    Pending,
    Rendered { svg: String },
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorState {
    pub head: usize,
    pub anchor: Option<usize>,
}

impl Default for CursorState {
    fn default() -> Self {
        Self {
            head: 0,
            anchor: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentBlock {
    pub id: usize,
    pub kind: BlockKind,
    pub source: String,
    pub rendered: String,
    pub html: Option<HtmlAdapter>,
    pub typst: Option<TypstAdapter>,
    pub draft: Option<String>,
    pub cursor: CursorState,
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

    pub fn focused_cursor(&self) -> Option<&CursorState> {
        self.focused_block_ref().map(|b| &b.cursor)
    }

    pub fn set_focused_cursor(&mut self, offset: usize, shift: bool) -> bool {
        let index = self.focused_block;
        let Some(text) = self.focused_text() else {
            return false;
        };
        let clamped = offset.min(text.len());

        let block = &mut self.blocks[index];
        let previous_head = block.cursor.head;
        let previous_anchor = block.cursor.anchor;

        if shift {
            if block.cursor.anchor.is_none() {
                block.cursor.anchor = Some(previous_head);
            }
            block.cursor.head = clamped;
            if block.cursor.anchor == Some(block.cursor.head) {
                block.cursor.anchor = None;
            }
        } else {
            block.cursor.head = clamped;
            block.cursor.anchor = None;
        }

        block.cursor.head != previous_head || block.cursor.anchor != previous_anchor
    }

    pub fn set_focused_draft(&mut self, draft: String) -> bool {
        let index = self.focused_block;
        let Some(block) = self.blocks.get_mut(index) else {
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
        self.blocks[index].draft = next_draft;

        // Ensure cursor is within bounds
        let text_len = self.focused_text().unwrap_or("").len();
        if self.blocks[index].cursor.head > text_len {
            self.blocks[index].cursor.head = text_len;
        }
        if let Some(anchor) = self.blocks[index].cursor.anchor {
            if anchor > text_len {
                self.blocks[index].cursor.anchor = Some(text_len);
            }
        }
        true
    }

    pub fn insert_text_at_cursor(&mut self, text_to_insert: &str) -> bool {
        let index = self.focused_block;
        let Some(current_text) = self.focused_text().map(ToOwned::to_owned) else {
            return false;
        };
        let cursor = self.blocks[index].cursor.clone();

        self.push_undo_snapshot();
        let (new_text, new_head) = if let Some(anchor) = cursor.anchor {
            let start = cursor.head.min(anchor);
            let end = cursor.head.max(anchor);
            let mut s = current_text;
            s.replace_range(start..end, text_to_insert);
            (s, start + text_to_insert.len())
        } else {
            let mut s = current_text;
            s.insert_str(cursor.head, text_to_insert);
            (s, cursor.head + text_to_insert.len())
        };

        let block = &mut self.blocks[index];
        block.cursor.head = new_head;
        block.cursor.anchor = None;
        block.draft = (new_text != block.source).then_some(new_text);
        true
    }

    pub fn append_to_focused_draft(&mut self, suffix: &str) -> bool {
        let index = self.focused_block;
        let Some(current) = self.focused_text() else {
            return false;
        };
        let end = current.len();

        self.blocks[index].cursor.head = end;
        self.blocks[index].cursor.anchor = None;
        self.insert_text_at_cursor(suffix)
    }

    pub fn push_char_to_focused_draft(&mut self, ch: char) -> bool {
        self.insert_text_at_cursor(&ch.to_string())
    }

    pub fn delete_at_cursor_in_focused_draft(&mut self) -> bool {
        let index = self.focused_block;
        let Some(text) = self.focused_text().map(ToOwned::to_owned) else {
            return false;
        };
        let cursor = self.blocks[index].cursor.clone();

        if cursor.anchor.is_none() && cursor.head == 0 {
            return false;
        }

        self.push_undo_snapshot();
        let (new_text, new_head) = if let Some(anchor) = cursor.anchor {
            let start = cursor.head.min(anchor);
            let end = cursor.head.max(anchor);
            let mut s = text;
            s.replace_range(start..end, "");
            (s, start)
        } else {
            let mut s = text;
            let Some((prev_idx, _)) = s[..cursor.head].char_indices().next_back() else {
                return false;
            };
            s.remove(prev_idx);
            (s, prev_idx)
        };

        let block = &mut self.blocks[index];
        block.cursor.head = new_head;
        block.cursor.anchor = None;
        block.draft = (new_text != block.source).then_some(new_text);
        true
    }

    pub fn delete_last_char_from_focused_draft(&mut self) -> bool {
        self.delete_at_cursor_in_focused_draft()
    }

    pub fn move_cursor_left(&mut self, shift: bool) -> bool {
        let index = self.focused_block;
        let head = self.blocks[index].cursor.head;
        let anchor = self.blocks[index].cursor.anchor;

        if !shift && anchor.is_some() {
            self.blocks[index].cursor.head = head.min(anchor.unwrap());
            self.blocks[index].cursor.anchor = None;
            return true;
        }

        let Some(text) = self.focused_text() else {
            return false;
        };
        let Some((prev_idx, _)) = text[..head].char_indices().next_back() else {
            return false;
        };

        let block = &mut self.blocks[index];
        if shift && block.cursor.anchor.is_none() {
            block.cursor.anchor = Some(head);
        }
        block.cursor.head = prev_idx;
        if !shift {
            block.cursor.anchor = None;
        }
        true
    }

    pub fn move_cursor_right(&mut self, shift: bool) -> bool {
        let index = self.focused_block;
        let head = self.blocks[index].cursor.head;
        let anchor = self.blocks[index].cursor.anchor;

        if !shift && anchor.is_some() {
            self.blocks[index].cursor.head = head.max(anchor.unwrap());
            self.blocks[index].cursor.anchor = None;
            return true;
        }

        let Some(text) = self.focused_text() else {
            return false;
        };
        if head >= text.len() {
            return false;
        }

        let ch = text[head..].chars().next().unwrap();
        let next_idx = head + ch.len_utf8();

        let block = &mut self.blocks[index];
        if shift && block.cursor.anchor.is_none() {
            block.cursor.anchor = Some(head);
        }
        block.cursor.head = next_idx;
        if !shift {
            block.cursor.anchor = None;
        }
        true
    }

    pub fn move_cursor_up(&mut self, shift: bool) -> bool {
        self.move_cursor_vertical(-1, shift)
    }

    pub fn move_cursor_down(&mut self, shift: bool) -> bool {
        self.move_cursor_vertical(1, shift)
    }

    pub fn select_all(&mut self) -> bool {
        let index = self.focused_block;
        let text_len = if let Some(text) = self.focused_text() {
            if text.is_empty() {
                return false;
            }
            text.len()
        } else {
            return false;
        };

        let block = &mut self.blocks[index];
        block.cursor.anchor = Some(0);
        block.cursor.head = text_len;
        true
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
        block.typst = typst_adapter_for_block(&block.kind, &block.rendered);

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
        self.blocks
            .insert(insert_at, new_block(insert_at, BlockKind::Paragraph, text));
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
        let mut duplicated = new_block(insert_at, block.kind, block.source);
        duplicated.typst = block.typst;
        self.blocks.insert(insert_at, duplicated);
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

    fn move_cursor_vertical(&mut self, delta: isize, shift: bool) -> bool {
        let index = self.focused_block;
        let Some(text) = self.focused_text() else {
            return false;
        };

        let lines = split_lines(text);
        let head = self.blocks[index].cursor.head;
        let Some((current_line_index, _current_line_start, current_column)) =
            line_position_for_offset(text, &lines, head)
        else {
            return false;
        };

        let Some(target_line_index) = current_line_index.checked_add_signed(delta) else {
            return false;
        };
        let Some(target_line) = lines.get(target_line_index).copied() else {
            return false;
        };

        let target_line_end = line_end(text, &lines, target_line_index);
        let target_offset =
            target_line + nth_char_offset(&text[target_line..target_line_end], current_column);

        self.set_focused_cursor(target_offset, shift)
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
            BlockKind::MathBlock => "math",
            BlockKind::TypstBlock => "typst",
        }
    }
}

fn parse_blocks(source: &str) -> Vec<DocumentBlock> {
    let mut blocks = Vec::new();
    let mut paragraph_lines: Vec<String> = Vec::new();
    let mut code_lines: Vec<String> = Vec::new();
    let mut code_language: Option<String> = None;
    let mut math_lines: Vec<String> = Vec::new();
    let mut in_math_block = false;

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
        blocks.push(new_block(id, BlockKind::Paragraph, content));
        paragraph_lines.clear();
    };

    for raw_line in source.lines() {
        let line = raw_line.trim_end();

        if in_math_block {
            if line.trim() == "$$" {
                let id = blocks.len();
                let math = math_lines.join("\n");
                blocks.push(new_block(
                    id,
                    BlockKind::MathBlock,
                    format!("$$\n{}\n$$", math),
                ));
                math_lines.clear();
                in_math_block = false;
                continue;
            }

            math_lines.push(line.to_string());
            continue;
        }

        if code_language.is_some() {
            if line.trim_start().starts_with("```") {
                let id = blocks.len();
                let code = code_lines.join("\n");
                let language = code_language.take();
                let (kind, source) = match language {
                    Some(language) if language == "typst" => {
                        (BlockKind::TypstBlock, format!("```typst\n{}\n```", code))
                    }
                    Some(language) => (
                        BlockKind::CodeFence {
                            language: Some(language.clone()),
                        },
                        format!("```{}\n{}\n```", language, code),
                    ),
                    None => (
                        BlockKind::CodeFence { language: None },
                        format!("```\n{}\n```", code),
                    ),
                };
                blocks.push(new_block(id, kind, source));
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
            let _ = text;
            blocks.push(new_block(
                id,
                BlockKind::Heading { level },
                line.to_string(),
            ));
            continue;
        }

        if let Some(text) = line.trim_start().strip_prefix("> ") {
            flush_paragraph(&mut blocks, &mut paragraph_lines);
            let id = blocks.len();
            let _ = text;
            blocks.push(new_block(id, BlockKind::Quote, line.to_string()));
            continue;
        }

        if let Some(text) = parse_unordered_list_item(line) {
            flush_paragraph(&mut blocks, &mut paragraph_lines);
            let id = blocks.len();
            let _ = text;
            blocks.push(new_block(
                id,
                BlockKind::ListItem { ordered: false },
                line.to_string(),
            ));
            continue;
        }

        if let Some(text) = parse_ordered_list_item(line) {
            flush_paragraph(&mut blocks, &mut paragraph_lines);
            let id = blocks.len();
            let _ = text;
            blocks.push(new_block(
                id,
                BlockKind::ListItem { ordered: true },
                line.to_string(),
            ));
            continue;
        }

        if let Some(math) = parse_single_line_math_block(line) {
            flush_paragraph(&mut blocks, &mut paragraph_lines);
            let id = blocks.len();
            let _ = math;
            blocks.push(new_block(id, BlockKind::MathBlock, line.trim().to_string()));
            continue;
        }

        if line.trim() == "$$" {
            flush_paragraph(&mut blocks, &mut paragraph_lines);
            in_math_block = true;
            math_lines.clear();
            continue;
        }

        paragraph_lines.push(line.to_string());
    }

    if code_language.is_some() {
        let id = blocks.len();
        let code = code_lines.join("\n");
        let language = code_language.take();
        let (kind, source) = match language {
            Some(language) if language == "typst" => {
                (BlockKind::TypstBlock, format!("```typst\n{}\n```", code))
            }
            Some(language) => (
                BlockKind::CodeFence {
                    language: Some(language.clone()),
                },
                format!("```{}\n{}\n```", language, code),
            ),
            None => (
                BlockKind::CodeFence { language: None },
                format!("```\n{}\n```", code),
            ),
        };
        blocks.push(new_block(id, kind, source));
    }

    if in_math_block {
        let id = blocks.len();
        let math = math_lines.join("\n");
        blocks.push(new_block(
            id,
            BlockKind::MathBlock,
            format!("$$\n{}\n$$", math),
        ));
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
            BlockKind::TypstBlock => stats.code_blocks += 1,
            BlockKind::MathBlock => {}
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

fn new_block(id: usize, kind: BlockKind, source: impl Into<String>) -> DocumentBlock {
    let source = source.into();
    let rendered = render_block_source(&kind, &source);
    let html = adapt_block_html(&kind, &source);
    let typst = typst_adapter_for_block(&kind, &rendered);

    DocumentBlock {
        id,
        kind,
        source,
        rendered,
        html,
        typst,
        draft: None,
        cursor: CursorState::default(),
    }
}

fn render_block_source(kind: &BlockKind, source: &str) -> String {
    match kind {
        BlockKind::Heading { .. } => parse_heading(source)
            .map(|(_, text)| text)
            .unwrap_or_else(|| source.trim().to_string()),
        BlockKind::Paragraph => render_paragraph_like_source(source),
        BlockKind::ListItem { ordered } => {
            if *ordered {
                render_paragraph_like_source(
                    parse_ordered_list_item(source).unwrap_or(source.trim()),
                )
            } else {
                render_paragraph_like_source(
                    parse_unordered_list_item(source).unwrap_or(source.trim()),
                )
            }
        }
        BlockKind::Quote => render_paragraph_like_source(
            source
                .trim_start()
                .strip_prefix("> ")
                .unwrap_or(source.trim()),
        ),
        BlockKind::CodeFence { .. } | BlockKind::TypstBlock => render_fenced_block_source(source),
        BlockKind::MathBlock => render_math_block_source(source),
    }
}

fn render_fenced_block_source(source: &str) -> String {
    source
        .lines()
        .skip(1)
        .take_while(|line| !line.trim_start().starts_with("```"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_math_block_source(source: &str) -> String {
    source
        .trim()
        .strip_prefix("$$")
        .and_then(|body| body.strip_suffix("$$"))
        .map(|body| body.trim().to_string())
        .unwrap_or_else(|| source.trim().to_string())
}

fn render_paragraph_like_source(source: &str) -> String {
    match adapt_html(source) {
        Some(HtmlAdapter::Adapted { ref nodes }) => summarize_html_nodes(nodes),
        _ => source.trim().to_string(),
    }
}

fn adapt_block_html(kind: &BlockKind, source: &str) -> Option<HtmlAdapter> {
    match kind {
        BlockKind::Paragraph | BlockKind::ListItem { .. } | BlockKind::Quote => adapt_html(source),
        BlockKind::Heading { .. }
        | BlockKind::CodeFence { .. }
        | BlockKind::MathBlock
        | BlockKind::TypstBlock => None,
    }
}

fn typst_adapter_for_block(kind: &BlockKind, rendered: &str) -> Option<TypstAdapter> {
    match kind {
        BlockKind::MathBlock | BlockKind::TypstBlock => Some(TypstAdapter::Pending),
        BlockKind::Paragraph | BlockKind::ListItem { .. } | BlockKind::Quote
            if contains_inline_math(rendered) =>
        {
            Some(TypstAdapter::Pending)
        }
        BlockKind::Heading { .. }
        | BlockKind::Paragraph
        | BlockKind::ListItem { .. }
        | BlockKind::Quote
        | BlockKind::CodeFence { .. } => None,
    }
}

fn adapt_html(source: &str) -> Option<HtmlAdapter> {
    if !source.contains('<') || !source.contains('>') {
        return None;
    }

    let mut remaining = source.trim();
    let mut nodes = Vec::new();
    let mut saw_html = false;

    while let Some(start) = remaining.find('<') {
        let before = &remaining[..start];
        if !before.is_empty() {
            nodes.push(HtmlNode::Text(before.to_string()));
        }

        let tag_start = &remaining[start..];
        if tag_start.starts_with("<img") {
            let Some(tag_end) = tag_start.find('>') else {
                return Some(HtmlAdapter::Unsupported {
                    raw: source.trim().to_string(),
                });
            };

            let tag = &tag_start[..=tag_end];
            let Some(image) = parse_img_tag(tag) else {
                return Some(HtmlAdapter::Unsupported {
                    raw: source.trim().to_string(),
                });
            };

            nodes.push(HtmlNode::Image(image));
            remaining = &tag_start[tag_end + 1..];
            saw_html = true;
            continue;
        }

        if tag_start.starts_with("<span") {
            let Some(open_end) = tag_start.find('>') else {
                return Some(HtmlAdapter::Unsupported {
                    raw: source.trim().to_string(),
                });
            };

            let open_tag = &tag_start[..=open_end];
            let after_open = &tag_start[open_end + 1..];
            let Some(close_start) = after_open.find("</span>") else {
                return Some(HtmlAdapter::Unsupported {
                    raw: source.trim().to_string(),
                });
            };

            let inner = &after_open[..close_start];
            if inner.contains('<') {
                return Some(HtmlAdapter::Unsupported {
                    raw: source.trim().to_string(),
                });
            }

            let (color, font_size_px) = parse_span_style(open_tag);
            nodes.push(HtmlNode::StyledText(HtmlStyledText {
                text: inner.to_string(),
                color,
                font_size_px,
            }));
            remaining = &after_open[close_start + "</span>".len()..];
            saw_html = true;
            continue;
        }

        return Some(HtmlAdapter::Unsupported {
            raw: source.trim().to_string(),
        });
    }

    if !remaining.is_empty() {
        nodes.push(HtmlNode::Text(remaining.to_string()));
    }

    saw_html.then_some(HtmlAdapter::Adapted { nodes })
}

fn summarize_html_nodes(nodes: &[HtmlNode]) -> String {
    nodes
        .iter()
        .map(|node| match node {
            HtmlNode::Text(text) => text.clone(),
            HtmlNode::StyledText(text) => text.text.clone(),
            HtmlNode::Image(image) => image
                .alt
                .clone()
                .or_else(|| image.src.clone())
                .map(|label| format!("[image: {label}]"))
                .unwrap_or_else(|| "[image]".to_string()),
        })
        .collect::<String>()
        .trim()
        .to_string()
}

fn parse_img_tag(tag: &str) -> Option<HtmlImage> {
    let attrs = parse_tag_attributes(tag);
    if attrs.is_empty() {
        return None;
    }

    let width_px = attrs
        .get("width")
        .and_then(|value| parse_px_u32(value).or_else(|| value.parse::<u32>().ok()));

    Some(HtmlImage {
        src: attrs.get("src").cloned(),
        alt: attrs.get("alt").cloned(),
        width_px,
    })
}

fn parse_span_style(tag: &str) -> (Option<String>, Option<u16>) {
    let attrs = parse_tag_attributes(tag);
    let Some(style) = attrs.get("style") else {
        return (None, None);
    };

    let mut color = None;
    let mut font_size_px = None;

    for declaration in style.split(';') {
        let Some((key, value)) = declaration.split_once(':') else {
            continue;
        };

        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();

        match key.as_str() {
            "color" if !value.is_empty() => color = Some(value.to_string()),
            "font-size" => {
                font_size_px = parse_px_u16(value);
            }
            _ => {}
        }
    }

    (color, font_size_px)
}

fn parse_tag_attributes(tag: &str) -> std::collections::BTreeMap<String, String> {
    let mut attrs = std::collections::BTreeMap::new();
    let mut inner = tag
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>')
        .trim_end_matches('/')
        .trim();

    let Some(first_space) = inner.find(char::is_whitespace) else {
        return attrs;
    };
    inner = inner[first_space..].trim();

    let bytes = inner.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }

        let key_start = index;
        while index < bytes.len() && !bytes[index].is_ascii_whitespace() && bytes[index] != b'=' {
            index += 1;
        }
        let key = inner[key_start..index].trim().to_ascii_lowercase();

        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() || bytes[index] != b'=' {
            while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            continue;
        }

        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }

        let value = if bytes[index] == b'"' || bytes[index] == b'\'' {
            let quote = bytes[index];
            index += 1;
            let value_start = index;
            while index < bytes.len() && bytes[index] != quote {
                index += 1;
            }
            let value = inner[value_start..index].to_string();
            if index < bytes.len() {
                index += 1;
            }
            value
        } else {
            let value_start = index;
            while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            inner[value_start..index].to_string()
        };

        if !key.is_empty() {
            attrs.insert(key, value);
        }
    }

    attrs
}

fn parse_px_u16(value: &str) -> Option<u16> {
    parse_px_u32(value).and_then(|value| value.try_into().ok())
}

fn parse_px_u32(value: &str) -> Option<u32> {
    let value = value.trim();
    let digits = value.strip_suffix("px").unwrap_or(value).trim();
    digits.parse::<u32>().ok()
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

fn parse_single_line_math_block(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    let body = trimmed.strip_prefix("$$")?.strip_suffix("$$")?.trim();
    (!body.is_empty()).then_some(body)
}

fn contains_inline_math(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'\\' {
            index += 2;
            continue;
        }

        if bytes[index] == b'$' {
            if index + 1 < bytes.len() && bytes[index + 1] == b'$' {
                index += 2;
                continue;
            }

            let start = index + 1;
            let mut inner = start;
            while inner < bytes.len() {
                if bytes[inner] == b'\\' {
                    inner += 2;
                    continue;
                }

                if bytes[inner] == b'$' {
                    if inner > start {
                        return true;
                    }
                    break;
                }

                inner += 1;
            }

            index = inner.saturating_add(1);
            continue;
        }

        index += 1;
    }

    false
}

fn split_lines(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (index, ch) in text.char_indices() {
        if ch == '\n' && index + ch.len_utf8() <= text.len() {
            starts.push(index + ch.len_utf8());
        }
    }
    starts
}

fn line_end(text: &str, line_starts: &[usize], line_index: usize) -> usize {
    let next_start = line_starts
        .get(line_index + 1)
        .copied()
        .unwrap_or(text.len());
    if next_start > 0 && text.as_bytes()[next_start.saturating_sub(1)] == b'\n' {
        next_start - 1
    } else {
        next_start
    }
}

fn line_position_for_offset(
    text: &str,
    line_starts: &[usize],
    offset: usize,
) -> Option<(usize, usize, usize)> {
    let clamped = offset.min(text.len());
    let line_index = line_starts
        .partition_point(|start| *start <= clamped)
        .saturating_sub(1);
    let line_start = *line_starts.get(line_index)?;
    let line_end = line_end(text, line_starts, line_index);
    let column_offset = clamped.min(line_end).saturating_sub(line_start);
    let column = text[line_start..line_start + column_offset].chars().count();
    Some((line_index, line_start, column))
}

fn nth_char_offset(text: &str, column: usize) -> usize {
    text.char_indices()
        .nth(column)
        .map(|(offset, _)| offset)
        .unwrap_or(text.len())
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
            block.rendered = render_block_source(&block.kind, &block.source);
            block.html = adapt_block_html(&block.kind, &block.source);
            let next_typst = typst_adapter_for_block(&block.kind, &block.rendered);
            block.typst = preserve_typst_adapter(block.typst.take(), next_typst);

            // Bounds check for cursor
            let text = block.draft.as_deref().unwrap_or(&block.source);
            let len = text.len();
            if block.cursor.head > len {
                block.cursor.head = len;
            }
            if let Some(anchor) = block.cursor.anchor {
                if anchor > len {
                    block.cursor.anchor = Some(len);
                }
            }
        }
        self.source = serialize_blocks(&self.blocks);
        self.outline = build_outline(&self.source);
        self.stats = build_stats(&self.blocks);
    }
}

fn preserve_typst_adapter(
    previous: Option<TypstAdapter>,
    next: Option<TypstAdapter>,
) -> Option<TypstAdapter> {
    match (previous, next) {
        (Some(previous), Some(TypstAdapter::Pending)) => Some(previous),
        (_, next) => next,
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
        // Move cursor to end before typing if we want to append
        assert!(document.move_cursor_right(false)); // P
        assert!(document.move_cursor_right(false)); // a
        assert!(document.move_cursor_right(false)); // r
        assert!(document.move_cursor_right(false)); // a
        assert!(document.move_cursor_right(false)); // g
        assert!(document.move_cursor_right(false)); // r
        assert!(document.move_cursor_right(false)); // a
        assert!(document.move_cursor_right(false)); // p
        assert!(document.move_cursor_right(false)); // h

        assert!(document.push_char_to_focused_draft('!'));
        assert_eq!(document.focused_text(), Some("Paragraph!"));
        assert!(document.delete_at_cursor_in_focused_draft());
        assert_eq!(document.focused_text(), Some("Paragraph"));
    }

    #[test]
    fn cursor_navigation_and_selection() {
        let mut document = DocumentModel::from_markdown("ABC");
        assert_eq!(document.focused_cursor().unwrap().head, 0);

        assert!(document.move_cursor_right(false));
        assert_eq!(document.focused_cursor().unwrap().head, 1);

        assert!(document.move_cursor_right(true)); // Select 'B'
        assert_eq!(document.focused_cursor().unwrap().head, 2);
        assert_eq!(document.focused_cursor().unwrap().anchor, Some(1));

        assert!(document.push_char_to_focused_draft('X')); // Replace 'B' with 'X'
        assert_eq!(document.focused_text(), Some("AXC"));
        assert_eq!(document.focused_cursor().unwrap().head, 2);
        assert_eq!(document.focused_cursor().unwrap().anchor, None);
    }

    #[test]
    fn select_all_works() {
        let mut document = DocumentModel::from_markdown("Hello");
        assert!(document.select_all());
        let cursor = document.focused_cursor().unwrap();
        assert_eq!(cursor.anchor, Some(0));
        assert_eq!(cursor.head, 5);
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
        assert!(document.append_to_focused_draft("!"));
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
        assert!(document.append_to_focused_draft("!"));
        assert!(document.undo());
        assert!(document.can_redo());

        assert!(document.append_to_focused_draft("?"));
        assert!(!document.can_redo());
        assert_eq!(document.focused_text(), Some("Paragraph?"));
    }

    #[test]
    fn html_adapter_extracts_safe_span_and_image_metadata() {
        let document = DocumentModel::from_markdown(
            "Intro <span style=\"color: #ff6600; font-size: 18px\">warm text</span> tail <img src=\"diagram.png\" alt=\"Diagram\" width=\"320\" />",
        );

        let Some(HtmlAdapter::Adapted { nodes }) = &document.blocks()[0].html else {
            panic!("expected adapted html metadata");
        };

        assert_eq!(
            document.blocks()[0].rendered,
            "Intro warm text tail [image: Diagram]"
        );
        assert!(matches!(&nodes[0], HtmlNode::Text(text) if text == "Intro "));
        assert!(matches!(
            &nodes[1],
            HtmlNode::StyledText(HtmlStyledText {
                text,
                color: Some(color),
                font_size_px: Some(18),
            }) if text == "warm text" && color == "#ff6600"
        ));
        assert!(matches!(
            &nodes[3],
            HtmlNode::Image(HtmlImage {
                src: Some(src),
                alt: Some(alt),
                width_px: Some(320),
            }) if src == "diagram.png" && alt == "Diagram"
        ));
    }

    #[test]
    fn unsupported_html_is_marked_for_degraded_preview() {
        let document = DocumentModel::from_markdown("<table><tr><td>cell</td></tr></table>");

        let Some(HtmlAdapter::Unsupported { raw }) = &document.blocks()[0].html else {
            panic!("expected unsupported html marker");
        };

        assert_eq!(raw, "<table><tr><td>cell</td></tr></table>");
        assert_eq!(
            document.blocks()[0].rendered,
            "<table><tr><td>cell</td></tr></table>"
        );
    }

    #[test]
    fn markdown_parser_recognizes_math_and_typst_blocks() {
        let document = DocumentModel::from_markdown(
            r#"$$
x^2 + y^2
$$

```typst
#set text(fill: red)
Hello
```"#,
        );

        assert_eq!(document.blocks().len(), 2);
        assert_eq!(document.blocks()[0].kind.label(), "math");
        assert_eq!(document.blocks()[0].rendered, "x^2 + y^2");
        assert_eq!(document.blocks()[1].kind.label(), "typst");
        assert_eq!(document.blocks()[1].rendered, "#set text(fill: red)\nHello");
    }

    #[test]
    fn markdown_parser_supports_single_line_math_blocks() {
        let document = DocumentModel::from_markdown("$$e^(i pi) + 1 = 0$$");

        assert_eq!(document.blocks().len(), 1);
        assert_eq!(document.blocks()[0].kind.label(), "math");
        assert_eq!(document.blocks()[0].rendered, "e^(i pi) + 1 = 0");
    }

    #[test]
    fn math_and_typst_blocks_start_with_pending_render_state() {
        let document = DocumentModel::from_markdown(
            r#"$$a + b$$

```typst
#let accent = blue
accent
```

```rust
fn main() {}
```"#,
        );

        assert!(matches!(
            document.blocks()[0].typst,
            Some(TypstAdapter::Pending)
        ));
        assert!(matches!(
            document.blocks()[1].typst,
            Some(TypstAdapter::Pending)
        ));
        assert!(document.blocks()[2].typst.is_none());
    }

    #[test]
    fn rebuild_metadata_reinitializes_pending_typst_state() {
        let mut document = DocumentModel::from_markdown("$$a + b$$");
        document.focused_block_mut().unwrap().typst = None;

        assert!(document.insert_paragraph_after_focused("tail paragraph"));
        assert!(matches!(
            document.blocks()[0].typst,
            Some(TypstAdapter::Pending)
        ));
        assert!(document.blocks()[1].typst.is_none());
    }

    #[test]
    fn inline_math_paragraph_like_blocks_start_with_pending_render_state() {
        let document = DocumentModel::from_markdown(
            r#"Paragraph with $a + b$ inline math.

- List item with $c + d$

> Quote with $e + f$

Plain paragraph without math."#,
        );

        assert!(matches!(
            document.blocks()[0].typst,
            Some(TypstAdapter::Pending)
        ));
        assert!(matches!(
            document.blocks()[1].typst,
            Some(TypstAdapter::Pending)
        ));
        assert!(matches!(
            document.blocks()[2].typst,
            Some(TypstAdapter::Pending)
        ));
        assert!(document.blocks()[3].typst.is_none());
    }

    #[test]
    fn rebuild_metadata_reinitializes_pending_inline_math_state() {
        let mut document = DocumentModel::from_markdown("Paragraph with $a + b$ inline math.");
        document.focused_block_mut().unwrap().typst = None;

        assert!(document.insert_paragraph_after_focused("tail paragraph"));
        assert!(matches!(
            document.blocks()[0].typst,
            Some(TypstAdapter::Pending)
        ));
        assert!(document.blocks()[1].typst.is_none());
    }

    #[test]
    fn rebuild_metadata_preserves_rendered_typst_state_when_source_is_unchanged() {
        let mut document = DocumentModel::from_markdown("$$a + b$$");
        document.focused_block_mut().unwrap().typst = Some(TypstAdapter::Rendered {
            svg: "<svg>stable</svg>".to_string(),
        });

        assert!(document.insert_paragraph_after_focused("tail paragraph"));
        assert!(matches!(
            document.blocks()[0].typst,
            Some(TypstAdapter::Rendered { ref svg }) if svg == "<svg>stable</svg>"
        ));
    }

    #[test]
    fn rebuild_metadata_preserves_error_typst_state_when_source_is_unchanged() {
        let mut document = DocumentModel::from_markdown("Paragraph with $a + b$ inline math.");
        document.focused_block_mut().unwrap().typst = Some(TypstAdapter::Error {
            message: "bad typst".to_string(),
        });

        assert!(document.insert_paragraph_after_focused("tail paragraph"));
        assert!(matches!(
            document.blocks()[0].typst,
            Some(TypstAdapter::Error { ref message }) if message == "bad typst"
        ));
    }

    #[test]
    fn duplicate_focused_block_preserves_rendered_typst_state() {
        let mut document = DocumentModel::from_markdown("$$a + b$$");
        document.focused_block_mut().unwrap().typst = Some(TypstAdapter::Rendered {
            svg: "<svg>stable</svg>".to_string(),
        });

        assert!(document.duplicate_focused_block());
        assert!(matches!(
            document.blocks()[1].typst,
            Some(TypstAdapter::Rendered { ref svg }) if svg == "<svg>stable</svg>"
        ));
    }

    #[test]
    fn duplicate_focused_block_preserves_error_typst_state() {
        let mut document = DocumentModel::from_markdown("Paragraph with $a + b$ inline math.");
        document.focused_block_mut().unwrap().typst = Some(TypstAdapter::Error {
            message: "bad typst".to_string(),
        });

        assert!(document.duplicate_focused_block());
        assert!(matches!(
            document.blocks()[1].typst,
            Some(TypstAdapter::Error { ref message }) if message == "bad typst"
        ));
    }

    #[test]
    fn escaped_dollar_does_not_trigger_inline_math_render_state() {
        let document = DocumentModel::from_markdown(r#"Price is \$42 today."#);

        assert!(document.blocks()[0].typst.is_none());
    }

    #[test]
    fn set_focused_cursor_clamps_to_text_bounds() {
        let mut document = DocumentModel::from_markdown("Hello");

        assert!(document.set_focused_cursor(999, false));
        let cursor = document.focused_cursor().unwrap();
        assert_eq!(cursor.head, 5);
        assert_eq!(cursor.anchor, None);
    }

    #[test]
    fn shift_set_focused_cursor_keeps_selection_anchor() {
        let mut document = DocumentModel::from_markdown("Hello");

        assert!(document.move_cursor_right(false));
        assert!(document.set_focused_cursor(4, true));

        let cursor = document.focused_cursor().unwrap();
        assert_eq!(cursor.head, 4);
        assert_eq!(cursor.anchor, Some(1));
    }

    #[test]
    fn cursor_moves_vertically_across_explicit_lines() {
        let mut document = DocumentModel::from_markdown("abc\ndef\nghij");

        assert!(document.move_cursor_right(false));
        assert!(document.move_cursor_right(false));
        assert_eq!(document.focused_cursor().unwrap().head, 2);

        assert!(document.move_cursor_down(false));
        assert_eq!(document.focused_cursor().unwrap().head, 6);

        assert!(document.move_cursor_down(false));
        assert_eq!(document.focused_cursor().unwrap().head, 10);

        assert!(document.move_cursor_up(false));
        assert_eq!(document.focused_cursor().unwrap().head, 6);
    }

    #[test]
    fn cursor_vertical_move_clamps_to_shorter_line_length() {
        let mut document = DocumentModel::from_markdown("ab\ncdef");

        assert!(document.move_cursor_down(false));
        assert!(document.move_cursor_right(false));
        assert!(document.move_cursor_right(false));
        assert!(document.move_cursor_right(false));
        assert_eq!(document.focused_cursor().unwrap().head, 6);

        assert!(document.move_cursor_up(false));
        assert_eq!(document.focused_cursor().unwrap().head, 2);
    }

    #[test]
    fn shift_cursor_vertical_move_extends_selection() {
        let mut document = DocumentModel::from_markdown("ab\ncd");

        assert!(document.move_cursor_right(false));
        assert!(document.move_cursor_down(true));

        let cursor = document.focused_cursor().unwrap();
        assert_eq!(cursor.head, 4);
        assert_eq!(cursor.anchor, Some(1));
    }
}
