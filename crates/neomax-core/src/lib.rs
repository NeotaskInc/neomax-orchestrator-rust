#[doc(hidden)]
#[macro_export]
macro_rules! __neomax_declare_domain_modules {
    (@declare generated $name:ident) => {
        pub mod $name;
    };
    (@declare existing $name:ident) => {};
    (
        $(
            $name:ident => {
                declaration: $declaration:ident,
                module: $module:literal,
                responsibility: $responsibility:literal,
                owners: [$($owner:literal),* $(,)?],
            }
        ),* $(,)?
    ) => {
        $($crate::__neomax_declare_domain_modules!(@declare $declaration $name);)*
    };
}

pub mod registry;
neomax_domain_declarations!(crate::__neomax_declare_domain_modules);

pub use concurrency::AdmissionSnapshot;
pub use config::{Engine, ModelDefaults, StatePaths, WorkerScope};
pub use error::{Error, Result};
pub use runtime::{ResolvedProviderExecutable, RuntimeEnvironment, RuntimePlatform};
pub use settings::{ConcurrencySettings, EffectiveSettings, SettingsFile};
