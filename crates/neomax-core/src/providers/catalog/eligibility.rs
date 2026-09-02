use super::types::ProviderSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eligibility {
    Eligible,
    MissingBinary,
    NotConnected,
    Unsupported,
}

pub fn orchestrator_eligibility(provider: &ProviderSnapshot) -> Eligibility {
    if !provider.spec.capabilities.orchestrator {
        return Eligibility::Unsupported;
    }
    if !provider.binary.available {
        return Eligibility::MissingBinary;
    }
    if !provider
        .profiles
        .iter()
        .any(|profile| profile.eligibility.orchestrator_eligible)
    {
        return Eligibility::NotConnected;
    }
    Eligibility::Eligible
}

pub fn worker_eligibility(provider: &ProviderSnapshot) -> Eligibility {
    if !provider.spec.capabilities.worker {
        return Eligibility::Unsupported;
    }
    if !provider.binary.available {
        return Eligibility::MissingBinary;
    }
    if !provider
        .profiles
        .iter()
        .any(|profile| profile.eligibility.worker_eligible)
    {
        return Eligibility::NotConnected;
    }
    Eligibility::Eligible
}
