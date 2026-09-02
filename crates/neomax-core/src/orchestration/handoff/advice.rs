use crate::Engine;
use crate::accounts::{RotationAdvice, rotation_advice as account_rotation_advice};

#[derive(Debug, Clone, PartialEq)]
pub struct HandoffAdvice {
    pub advised: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HandoffCheck {
    pub engine: Engine,
    pub account: String,
    pub five_hour: f64,
    pub seven_day: f64,
    pub advice: HandoffAdvice,
    pub target_account: Option<String>,
    pub target_weekly_resets: Option<String>,
    pub target_email: Option<String>,
}

impl HandoffCheck {
    pub const ROTATE_EXIT: i32 = 10;

    pub fn exit_code(&self) -> i32 {
        if self.advice.advised {
            Self::ROTATE_EXIT
        } else {
            0
        }
    }
}

pub fn rotation_advice(engine: Engine, five_hour: f64, seven_day: f64) -> HandoffAdvice {
    let RotationAdvice { rotate, reason } = account_rotation_advice(engine, five_hour, seven_day);
    HandoffAdvice {
        advised: rotate,
        reason,
    }
}

pub fn check_result(
    engine: Engine,
    account: impl Into<String>,
    five_hour: f64,
    seven_day: f64,
    target_account: Option<String>,
    target_weekly_resets: Option<String>,
    target_email: Option<String>,
) -> HandoffCheck {
    HandoffCheck {
        engine,
        account: account.into(),
        five_hour,
        seven_day,
        advice: rotation_advice(engine, five_hour, seven_day),
        target_account,
        target_weekly_resets,
        target_email,
    }
}
