use crate::Engine;
use crate::accounts;

use super::*;

#[test]
fn rotation_policy_uses_provider_windows_and_hard_wall() {
    assert!(!rotation_advice(Engine::Claude, 98.0, 20.0).rotate);
    assert_eq!(
        rotation_advice(Engine::Claude, 99.0, 20.0).window,
        Some(RotationWindow::FiveHour)
    );
    assert_eq!(
        rotation_advice(Engine::Claude, 20.0, 99.0).window,
        Some(RotationWindow::Weekly)
    );
    assert!(!rotation_advice(Engine::Codex, 99.0, 20.0).rotate);
    assert_eq!(
        rotation_advice(Engine::Codex, 1.0, accounts::WEEKLY_HARD_PERCENT).window,
        Some(RotationWindow::Weekly)
    );
}
