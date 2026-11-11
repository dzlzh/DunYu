//! Explicit index transitions keep identical-looking rules independent.
use crate::SummonRule;
#[derive(Clone, Copy, Debug)]
pub enum RuleChange {
    Unchanged,
    Append,
    Update(usize),
    Delete(usize),
}

/// Validate the edit before any external effects; return old/new retained indices.
pub fn retained(
    old: &[SummonRule],
    new: &[SummonRule],
    change: RuleChange,
) -> Result<Vec<(usize, usize)>, String> {
    let pairs: Vec<_> = match change {
        RuleChange::Unchanged if old == new => (0..old.len()).map(|i| (i, i)).collect(),
        RuleChange::Append if new.len() == old.len() + 1 && new[..old.len()] == *old => {
            (0..old.len()).map(|i| (i, i)).collect()
        }
        RuleChange::Update(index)
            if index < old.len()
                && old.len() == new.len()
                && old
                    .iter()
                    .zip(new)
                    .enumerate()
                    .all(|(i, (a, b))| i == index || a == b) =>
        {
            (0..old.len()).map(|i| (i, i)).collect()
        }
        RuleChange::Delete(index)
            if index < old.len()
                && old.len() == new.len() + 1
                && old
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != index)
                    .map(|(_, r)| r)
                    .eq(new.iter()) =>
        {
            (0..old.len())
                .filter(|i| *i != index)
                .map(|i| (i, if i > index { i - 1 } else { i }))
                .collect()
        }
        _ => return Err("规则变更与当前列表不一致".into()),
    };
    Ok(pairs
        .into_iter()
        .filter(|&(a, b)| {
            old[a].process_path.to_lowercase() == new[b].process_path.to_lowercase()
                && old[a].class == new[b].class
                && old[a].title == new[b].title
        })
        .collect())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn rule() -> SummonRule {
        SummonRule {
            name: "same".into(),
            enabled: true,
            executable: "app.exe".into(),
            arguments: String::new(),
            working_directory: String::new(),
            process_path: "C:\\App.exe".into(),
            class: String::new(),
            title: String::new(),
            hotkey: crate::Hotkey::new(3, 65),
            minimize: false,
            priority_hotkey: false,
        }
    }
    #[test]
    fn edits_and_identical_rules_keep_identity() {
        let old = vec![rule(); 3];
        let mut new = old.clone();
        new[1].name = "renamed".into();
        new[1].enabled = false;
        new[1].arguments = "--new".into();
        new[1].hotkey.key = 66;
        new[1].minimize = true;
        new[1].priority_hotkey = true;
        new[1].process_path = old[1].process_path.to_uppercase();
        assert_eq!(
            retained(&old, &new, RuleChange::Update(1)).unwrap(),
            vec![(0, 0), (1, 1), (2, 2)]
        );
        new[1].title = "changed".into();
        assert_eq!(
            retained(&old, &new, RuleChange::Update(1)).unwrap(),
            vec![(0, 0), (2, 2)]
        );
        for i in 0..3 {
            let mut new = old.clone();
            new.remove(i);
            let pairs = retained(&old, &new, RuleChange::Delete(i)).unwrap();
            assert_eq!(pairs.len(), 2);
            assert!(!pairs.iter().any(|p| p.0 == i));
            assert_eq!(pairs.iter().map(|p| p.1).collect::<Vec<_>>(), vec![0, 1]);
        }
        let mut new = old.clone();
        new.push(rule());
        assert_eq!(retained(&old, &new, RuleChange::Append).unwrap().len(), 3);
        assert!(retained(&old, &new, RuleChange::Unchanged).is_err());
        assert!(retained(&old, &old, RuleChange::Update(3)).is_err());
        assert!(retained(&old, &old, RuleChange::Delete(0)).is_err());
    }
}
