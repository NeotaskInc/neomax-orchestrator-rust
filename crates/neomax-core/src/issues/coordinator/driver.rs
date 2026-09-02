use crate::Result;

use super::super::types::{IssueMirror, MirrorState};
use super::types::{MirrorRequest, RepositoryTarget};

pub trait MirrorDriver: Send + Sync {
    fn create(&self, target: &RepositoryTarget, request: &MirrorRequest) -> Result<IssueMirror>;

    fn comment(&self, target: &RepositoryTarget, mirror: &IssueMirror, text: &str) -> Result<()>;

    fn close(
        &self,
        target: &RepositoryTarget,
        mirror: &IssueMirror,
        comment: Option<&str>,
    ) -> Result<()>;

    fn state(&self, target: &RepositoryTarget, mirror: &IssueMirror)
        -> Result<Option<MirrorState>>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LocalOnlyMirrorDriver;

impl MirrorDriver for LocalOnlyMirrorDriver {
    fn create(&self, _target: &RepositoryTarget, _request: &MirrorRequest) -> Result<IssueMirror> {
        Ok(IssueMirror::local())
    }

    fn comment(
        &self,
        _target: &RepositoryTarget,
        _mirror: &IssueMirror,
        _text: &str,
    ) -> Result<()> {
        Ok(())
    }

    fn close(
        &self,
        _target: &RepositoryTarget,
        _mirror: &IssueMirror,
        _comment: Option<&str>,
    ) -> Result<()> {
        Ok(())
    }

    fn state(
        &self,
        _target: &RepositoryTarget,
        _mirror: &IssueMirror,
    ) -> Result<Option<MirrorState>> {
        Ok(None)
    }
}
