use thiserror::Error;
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime};
use typst::syntax::{FileId, Source};
use typst::text::{Font, FontBook};
use typst::Library;
use typst::World;
use typst::utils::LazyHash;
use typst::layout::PagedDocument;

#[derive(Error, Debug)]
pub enum TypstError {
    #[error("Compilation failed: {0}")]
    Compile(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

pub enum RenderKind {
    Math,
    Block,
}

struct SolaWorld {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    source: Source,
}

impl SolaWorld {
    fn new(text: &str) -> Self {
        let library = LazyHash::new(Library::builder().build());
        let fonts: Vec<Font> = typst_assets::fonts()
            .map(|data| Font::new(Bytes::new(data.to_vec()), 0).unwrap())
            .collect();
        let book = LazyHash::new(FontBook::from_fonts(&fonts));
        let source = Source::detached(text);

        Self {
            library,
            book,
            fonts,
            source,
        }
    }
}

impl World for SolaWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }
    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }
    fn main(&self) -> FileId {
        self.source.id()
    }
    fn source(&self, _id: FileId) -> FileResult<Source> {
        Ok(self.source.clone())
    }
    fn file(&self, _id: FileId) -> FileResult<Bytes> {
        Err(FileError::NotFound(
            _id.vpath().as_rootless_path().to_path_buf(),
        ))
    }
    fn font(&self, id: usize) -> Option<Font> {
        self.fonts.get(id).cloned()
    }
    fn today(&self, _offset: Option<i64>) -> Option<Datetime> {
        None
    }
}

pub fn compile_to_svg(source: &str, kind: RenderKind) -> Result<String, TypstError> {
    let full_source = match kind {
        RenderKind::Math => format!(
            "#set page(width: auto, height: auto, margin: 0pt)\n${}$",
            source
        ),
        RenderKind::Block => source.to_string(),
    };

    let world = SolaWorld::new(&full_source);
    let document: PagedDocument = typst::compile(&world)
        .output
        .map_err(|err| TypstError::Compile(format!("{:?}", err)))?;

    let svg = typst_svg::svg(&document.pages[0]);
    Ok(svg)
}
