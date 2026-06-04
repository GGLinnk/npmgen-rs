//! Crate-facade error wrapping each phase's typed error and the pipeline's own
//! preconditions, with the source chain preserved.

/// Any failure of the generation pipeline.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Project(#[from] crate::project::ProjectError),

    #[error(transparent)]
    Target(#[from] crate::target::TargetError),

    #[error(transparent)]
    Compile(#[from] crate::compile::CompileError),

    #[error(transparent)]
    Npm(#[from] crate::npm::NpmError),

    #[error("git tag {tag} does not match the package version {expected}")]
    TagMismatch { tag: String, expected: String },
}

pub type Result<T> = std::result::Result<T, Error>;
