use std::cell::RefCell;
use tree_sitter::{Language, Parser, Query, QueryCursor, StreamingIterator};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightKind {
    Keyword,
    String,
    Comment,
    Function,
    Number,
    Constant,
    TypeName,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightedSpan {
    pub text: String,
    pub kind: HighlightKind,
}

pub struct SyntaxHighlighter {
    parser: RefCell<Parser>,
    query: Query,
}

impl SyntaxHighlighter {
    pub fn new_rust() -> Self {
        let language: Language = tree_sitter_rust::LANGUAGE.into();
        let mut parser = Parser::new();
        parser
            .set_language(&language)
            .expect("Error loading Rust grammar");

        // Basic query for Rust syntax highlighting
        let query = Query::new(
            &language,
            r#"
            [
              "use" "mod" "fn" "struct" "enum" "impl" "trait" "let" "const"
              "static" "if" "else" "match" "loop" "while" "for" "in" "return" "break" "continue"
            ] @keyword

            (string_literal) @string
            (line_comment) @comment
            (block_comment) @comment
            (function_item name: (identifier) @function)
            (call_expression function: (identifier) @function)
            (integer_literal) @number
            (type_identifier) @type_name
            "#,
        )
        .expect("Error loading Rust query");

        Self {
            parser: RefCell::new(parser),
            query,
        }
    }

    pub fn highlight(&self, text: &str) -> Vec<HighlightedSpan> {
        let mut parser = self.parser.borrow_mut();
        let tree = parser.parse(text, None).expect("Error parsing text");
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&self.query, tree.root_node(), text.as_bytes());

        let mut spans = Vec::new();
        let mut last_index = 0;

        // Collect all captures and sort them by start index
        let mut captures = Vec::new();
        while let Some(m) = matches.next() {
            for capture in m.captures {
                captures.push(*capture);
            }
        }
        captures.sort_by_key(|c| c.node.start_byte());

        for capture in captures {
            let start = capture.node.start_byte();
            let end = capture.node.end_byte();

            if start > last_index {
                spans.push(HighlightedSpan {
                    text: text[last_index..start].to_string(),
                    kind: HighlightKind::Other,
                });
            }

            if start < last_index {
                continue; // Skip overlapping captures for simplicity in prototype
            }

            let kind = match self.query.capture_names()[capture.index as usize] {
                "keyword" => HighlightKind::Keyword,
                "string" => HighlightKind::String,
                "comment" => HighlightKind::Comment,
                "function" => HighlightKind::Function,
                "number" => HighlightKind::Number,
                "constant" => HighlightKind::Constant,
                "type_name" => HighlightKind::TypeName,
                _ => HighlightKind::Other,
            };

            spans.push(HighlightedSpan {
                text: text[start..end].to_string(),
                kind,
            });

            last_index = end;
        }

        if last_index < text.len() {
            spans.push(HighlightedSpan {
                text: text[last_index..].to_string(),
                kind: HighlightKind::Other,
            });
        }

        spans
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_highlighter_extracts_keywords_and_strings() {
        let highlighter = SyntaxHighlighter::new_rust();
        let code = r#"fn main() { let x = "hello"; }"#;
        let spans = highlighter.highlight(code);

        assert!(
            spans
                .iter()
                .any(|s| s.kind == HighlightKind::Keyword && s.text == "fn")
        );
        assert!(
            spans
                .iter()
                .any(|s| s.kind == HighlightKind::Keyword && s.text == "let")
        );
        assert!(
            spans
                .iter()
                .any(|s| s.kind == HighlightKind::String && s.text == "\"hello\"")
        );
        assert!(
            spans
                .iter()
                .any(|s| s.kind == HighlightKind::Function && s.text == "main")
        );
    }
}
