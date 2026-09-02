use neomax_core::Engine;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResumeTarget {
    pub engine: Engine,
    pub account: String,
    pub session_id: String,
}
