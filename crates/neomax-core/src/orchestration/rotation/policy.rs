use crate::Engine;
use crate::accounts;

use super::types::{RotationAdvice, RotationWindow};

pub const fn engine_has_five_hour(engine: Engine) -> bool {
    accounts::engine_has_five_hour(engine)
}

pub fn rotation_advice(engine: Engine, five_hour: f64, weekly: f64) -> RotationAdvice {
    let advice = accounts::rotation_advice(engine, five_hour, weekly);
    let window = if advice.rotate {
        if engine_has_five_hour(engine) && five_hour >= accounts::LIVE_ROTATION_FIVE_PERCENT {
            Some(RotationWindow::FiveHour)
        } else {
            Some(RotationWindow::Weekly)
        }
    } else {
        None
    };
    RotationAdvice {
        rotate: advice.rotate,
        reason: advice.reason,
        window,
    }
}

pub fn rotation_window(engine: Engine, five_hour: f64, weekly: f64) -> Option<RotationWindow> {
    rotation_advice(engine, five_hour, weekly).window
}
