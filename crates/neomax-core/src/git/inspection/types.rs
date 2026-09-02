use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitInspectionRequest {
    pub repository: PathBuf,
    pub branch: Option<String>,
    pub base: Option<String>,
}

impl GitInspectionRequest {
    pub fn new(repository: impl Into<PathBuf>) -> Self {
        Self {
            repository: repository.into(),
            branch: None,
            base: None,
        }
    }

    pub fn branch(mut self, branch: impl Into<String>) -> Self {
        self.branch = Some(branch.into());
        self
    }

    pub fn base(mut self, base: impl Into<String>) -> Self {
        self.base = Some(base.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitInspection {
    pub repository_root: PathBuf,
    pub branch: String,
    pub base: String,
    pub base_ref: String,
    pub head_sha: String,
    pub base_sha: String,
    pub branch_is_ancestor_of_base: bool,
    pub ahead: u64,
}
