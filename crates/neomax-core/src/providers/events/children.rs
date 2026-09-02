use crate::providers::ChildActivity;

pub(super) fn upsert(children: &mut Vec<ChildActivity>, child: ChildActivity) {
    if let Some(existing) = children.iter_mut().find(|existing| existing.id == child.id) {
        *existing = child;
    } else {
        children.push(child);
    }
}

pub(super) fn close_running(children: &mut [ChildActivity], status: &str) {
    for child in children
        .iter_mut()
        .filter(|child| child.status == "running")
    {
        child.status = status.into();
    }
}
