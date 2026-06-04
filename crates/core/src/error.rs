//! Crate-facade error wrapping each phase's typed error and the pipeline's own
//! preconditions, with the source chain preserved. The per-phase errors are
//! boxed so the facade (and the `Result` it travels in) stays small.

/// Any failure of the generation pipeline.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Project(Box<crate::project::ProjectError>),

    #[error(transparent)]
    Target(Box<crate::target::TargetError>),

    #[error(transparent)]
    Compile(Box<crate::compile::CompileError>),

    #[error(transparent)]
    Npm(Box<crate::npm::NpmError>),

    #[error("git tag {tag} does not match the package version {expected}")]
    TagMismatch { tag: String, expected: String },
}

impl From<crate::project::ProjectError> for Error {
    fn from(error: crate::project::ProjectError) -> Self {
        Self::Project(Box::new(error))
    }
}

impl From<crate::target::TargetError> for Error {
    fn from(error: crate::target::TargetError) -> Self {
        Self::Target(Box::new(error))
    }
}

impl From<crate::compile::CompileError> for Error {
    fn from(error: crate::compile::CompileError) -> Self {
        Self::Compile(Box::new(error))
    }
}

impl From<crate::npm::NpmError> for Error {
    fn from(error: crate::npm::NpmError) -> Self {
        Self::Npm(Box::new(error))
    }
}

pub type Result<T> = std::result::Result<T, Error>;
