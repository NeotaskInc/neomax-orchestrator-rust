use super::QueueState;

pub fn allocate(state: &mut QueueState) {
    let mut free = state.agent_budget.saturating_sub(used(state));
    for reservation in &mut state.queue {
        if free == 0 {
            break;
        }
        if reservation.granted > 0 && reservation.granted < reservation.want {
            let additional = reservation
                .want
                .saturating_sub(reservation.granted)
                .min(free);
            reservation.granted += additional;
            free -= additional;
        }
    }
    let mut active = u32::try_from(
        state
            .queue
            .iter()
            .filter(|reservation| reservation.granted > 0)
            .count(),
    )
    .unwrap_or(u32::MAX);
    free = state.agent_budget.saturating_sub(used(state));
    for reservation in &mut state.queue {
        if free == 0 {
            break;
        }
        if reservation.granted == 0 && reservation.want > 0 {
            if state.task_budget != 0 && active >= state.task_budget {
                break;
            }
            reservation.granted = reservation.want.min(free);
            free -= reservation.granted;
            if reservation.granted > 0 {
                active = active.saturating_add(1);
            }
        }
    }
}

fn used(state: &QueueState) -> u32 {
    state
        .queue
        .iter()
        .map(|reservation| reservation.granted.min(reservation.want))
        .fold(0, u32::saturating_add)
}
