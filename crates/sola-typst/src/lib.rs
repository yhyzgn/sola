use std::sync::Arc;
use thiserror::Error;

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

pub fn compile_to_svg(source: &str, kind: RenderKind) -> Result<String, TypstError> {
    // Stub implementation
    Ok("<svg></svg>".to_string())
}
