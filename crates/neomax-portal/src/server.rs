mod actions;
mod connection;
mod response;
#[cfg(test)]
mod tests;

use crate::actions::{
    GhPrStateResolver, LocalActionExecutor, PrStateResolver, SystemActionExecutor,
};
use crate::address::LocalBind;
use crate::source::PortalSource;

pub struct PortalServer<S, E = SystemActionExecutor, P = GhPrStateResolver> {
    bind: LocalBind,
    source: S,
    executor: E,
    pr_state: P,
    default_days: u32,
}

impl<S> PortalServer<S, SystemActionExecutor, GhPrStateResolver>
where
    S: PortalSource,
{
    pub fn new(bind: LocalBind, source: S) -> Self {
        Self {
            bind,
            source,
            executor: SystemActionExecutor,
            pr_state: GhPrStateResolver,
            default_days: 30,
        }
    }
}

impl<S, E, P> PortalServer<S, E, P>
where
    S: PortalSource + 'static,
    E: LocalActionExecutor + 'static,
    P: PrStateResolver + 'static,
{
    pub fn with_action_executor<E2>(self, executor: E2) -> PortalServer<S, E2, P>
    where
        E2: LocalActionExecutor,
    {
        PortalServer {
            bind: self.bind,
            source: self.source,
            executor,
            pr_state: self.pr_state,
            default_days: self.default_days,
        }
    }

    pub fn with_pr_state_resolver<P2>(self, pr_state: P2) -> PortalServer<S, E, P2>
    where
        P2: PrStateResolver,
    {
        PortalServer {
            bind: self.bind,
            source: self.source,
            executor: self.executor,
            pr_state,
            default_days: self.default_days,
        }
    }

    pub fn with_days(mut self, days: u32) -> Self {
        self.default_days = days.min(3660);
        self
    }
}
