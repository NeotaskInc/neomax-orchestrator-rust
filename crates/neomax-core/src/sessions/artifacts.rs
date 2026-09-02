mod encoding;
mod filesystem;
mod index;
mod matching;
mod memory;
mod source;
mod types;

#[cfg(test)]
mod tests;

pub use encoding::{flatten_extra, json_lines, json_object};
pub use filesystem::FsArtifactSource;
pub use index::{ArtifactIndex, ProviderArtifactIndex};
pub use memory::MemoryArtifactSource;
pub use source::ArtifactSource;
pub use types::{artifact, engine_for_kind, Artifact, ArtifactKind, ArtifactLocator};
