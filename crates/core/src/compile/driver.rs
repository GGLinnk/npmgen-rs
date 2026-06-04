use crate::project::Project;
use crate::target::Target;

use super::CompileError;

/// A backend that builds one target's binary. The default is [`CargoDriver`](super::CargoDriver);
/// implementing this trait swaps in another toolchain or a test double.
pub trait BuildDriver: std::fmt::Debug {
    fn build(&self, project: &Project, target: &Target) -> Result<(), CompileError>;
}
