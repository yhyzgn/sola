use pulldown_cmark::{Options, Parser, html};
use sola_document::DocumentModel;
use sola_theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Markdown,
    Html,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportArtifact {
    pub format: ExportFormat,
    pub mime_type: &'static str,
    pub extension: &'static str,
    pub bytes: Vec<u8>,
}

pub fn export_document(
    document: &DocumentModel,
    theme: &Theme,
    format: ExportFormat,
) -> ExportArtifact {
    match format {
        ExportFormat::Markdown => ExportArtifact {
            format,
            mime_type: "text/markdown; charset=utf-8",
            extension: "md",
            bytes: document.source().as_bytes().to_vec(),
        },
        ExportFormat::Html => {
            let mut body = String::new();
            html::push_html(
                &mut body,
                Parser::new_ext(document.source(), Options::all()),
            );

            let html = format!(
                r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Sola Export</title>
  <style>
    :root {{
      --app-background: {app_background};
      --panel-background: {panel_background};
      --panel-border: {panel_border};
      --text-primary: {text_primary};
      --text-muted: {text_muted};
      --accent: {accent};
      --code-background: {code_background};
      --selection: {selection};
      --cursor: {cursor};
      --body-size: {body_size}px;
      --title-size: {title_size}px;
      --code-size: {code_size}px;
    }}

    * {{
      box-sizing: border-box;
    }}

    body {{
      margin: 0;
      padding: 48px 24px;
      background: var(--app-background);
      color: var(--text-primary);
      font-size: var(--body-size);
      line-height: 1.6;
      font-family: "Iowan Old Style", "Palatino Linotype", "Book Antiqua", Georgia, serif;
    }}

    main {{
      max-width: 840px;
      margin: 0 auto;
    }}

    h1, h2, h3, h4, h5, h6 {{
      color: var(--text-primary);
      line-height: 1.25;
      margin-top: 1.6em;
      margin-bottom: 0.6em;
    }}

    h1 {{
      font-size: calc(var(--title-size) * 1.4);
    }}

    h2 {{
      font-size: calc(var(--title-size) * 1.15);
    }}

    h3 {{
      font-size: var(--title-size);
    }}

    p, li, blockquote {{
      color: var(--text-primary);
    }}

    blockquote {{
      margin: 1.25em 0;
      padding-left: 16px;
      border-left: 3px solid var(--accent);
      color: var(--text-muted);
    }}

    a {{
      color: var(--accent);
    }}

    code, pre {{
      font-family: "Berkeley Mono", "JetBrains Mono", "SFMono-Regular", Consolas, monospace;
      font-size: var(--code-size);
    }}

    code {{
      padding: 0.15em 0.35em;
      border-radius: 6px;
      background: var(--panel-background);
    }}

    pre {{
      overflow-x: auto;
      padding: 16px;
      border: 1px solid var(--panel-border);
      border-radius: 14px;
      background: var(--code-background);
    }}

    pre code {{
      padding: 0;
      background: transparent;
    }}

    img {{
      max-width: 100%;
      height: auto;
    }}

    hr {{
      border: 0;
      border-top: 1px solid var(--panel-border);
      margin: 2em 0;
    }}
  </style>
</head>
<body>
  <main>
    {body}
  </main>
</body>
</html>
"#,
                app_background = theme.palette.app_background,
                panel_background = theme.palette.panel_background,
                panel_border = theme.palette.panel_border,
                text_primary = theme.palette.text_primary,
                text_muted = theme.palette.text_muted,
                accent = theme.palette.accent,
                code_background = theme.palette.code_background,
                selection = theme.palette.selection,
                cursor = theme.palette.cursor,
                body_size = theme.typography.body_size,
                title_size = theme.typography.title_size,
                code_size = theme.typography.code_size,
                body = body,
            );

            ExportArtifact {
                format,
                mime_type: "text/html; charset=utf-8",
                extension: "html",
                bytes: html.into_bytes(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_export_returns_current_document_source() {
        let document = DocumentModel::from_markdown("# Title\n\nParagraph");
        let artifact = export_document(&document, &Theme::sola_dark(), ExportFormat::Markdown);

        assert_eq!(artifact.format, ExportFormat::Markdown);
        assert_eq!(artifact.mime_type, "text/markdown; charset=utf-8");
        assert_eq!(artifact.extension, "md");
        assert_eq!(
            String::from_utf8(artifact.bytes).unwrap(),
            "# Title\n\nParagraph"
        );
    }

    #[test]
    fn html_export_wraps_markdown_body_with_theme_styles() {
        let document = DocumentModel::from_markdown(
            "# Title\n\nParagraph with `code`.\n\n```rust\nfn main() {}\n```",
        );
        let artifact = export_document(&document, &Theme::sola_dark(), ExportFormat::Html);
        let html = String::from_utf8(artifact.bytes).unwrap();

        assert_eq!(artifact.format, ExportFormat::Html);
        assert_eq!(artifact.mime_type, "text/html; charset=utf-8");
        assert_eq!(artifact.extension, "html");
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("<h1>Title</h1>"));
        assert!(html.contains("Paragraph with <code>code</code>."));
        assert!(html.contains("language-rust"));
        assert!(html.contains(&Theme::sola_dark().palette.app_background));
    }
}
