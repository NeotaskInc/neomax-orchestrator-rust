use std::collections::BTreeMap;

use neomax_core::sessions::SessionRecord;

pub(crate) fn ambient_records(
    records: Vec<SessionRecord>,
) -> (Vec<SessionRecord>, Vec<SessionRecord>) {
    let mut mains = records
        .iter()
        .filter(|record| !record.is_child())
        .map(|record| (record.id.clone(), record.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut children = Vec::new();
    for record in records.into_iter().filter(|record| record.is_child()) {
        if let Some(parent) = record.parent_id.as_ref().and_then(|id| mains.get_mut(id)) {
            parent.children.push(record.clone());
        }
        children.push(record);
    }
    let mut mains = mains.into_values().collect::<Vec<_>>();
    mains.sort_by_key(|record| std::cmp::Reverse(record.last_active.unwrap_or_default()));
    (mains, children)
}

#[cfg(test)]
mod tests {
    use super::*;
    use neomax_core::config::Engine;
    use neomax_core::sessions::SessionKind;

    #[test]
    fn ambient_children_are_attached_to_their_main_session() {
        let main = SessionRecord::with_identity("main", Engine::Claude, "1");
        let mut child = SessionRecord::with_identity("child", Engine::Claude, "1");
        child.kind = SessionKind::NativeSubagent;
        child.parent_id = Some("main".into());
        let (mains, children) = ambient_records(vec![main, child]);
        assert_eq!(mains.len(), 1);
        assert_eq!(mains[0].children.len(), 1);
        assert_eq!(children.len(), 1);
    }
}
