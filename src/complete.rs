use crate::model::{tilde, untilde};
use crate::view::Candidate;
use std::path::Path;

pub fn candidates(typed: &str, limit: usize) -> Vec<Candidate> {
    let (dir, prefix) = match typed.rfind('/') {
        Some(i) => (&typed[..=i], &typed[i + 1..]),
        None => ("./", typed),
    };
    let base = untilde(dir);
    let Ok(entries) = std::fs::read_dir(&base) else {
        return Vec::new();
    };
    let lower = prefix.to_lowercase();
    let mut found: Vec<(std::path::PathBuf, std::time::SystemTime)> = entries
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter(|e| {
            let n = e.file_name().to_string_lossy().to_lowercase();
            n.starts_with(&lower) && (prefix.starts_with('.') || !n.starts_with('.'))
        })
        .filter_map(|e| Some((e.path(), e.metadata().ok()?.modified().ok()?)))
        .collect();
    found.sort_by_key(|b| std::cmp::Reverse(b.1));
    found
        .into_iter()
        .take(limit)
        .map(|(p, _)| Candidate {
            path: tilde(&p),
            info: crate::git::describe(&p),
        })
        .collect()
}

pub fn name_for(path: &str) -> String {
    let p = path.trim_end_matches('/');
    crate::model::folder_name(Path::new(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_match() {
        let tmp = tempfile::tempdir().unwrap();
        for d in ["billing-app", "billing-app-legacy", "other", ".hidden"] {
            std::fs::create_dir(tmp.path().join(d)).unwrap();
        }
        let typed = format!("{}/billing", tmp.path().display());
        let c = candidates(&typed, 4);
        assert_eq!(c.len(), 2);
        assert!(c.iter().all(|c| c.path.contains("billing-app")));
        let all = candidates(&format!("{}/", tmp.path().display()), 10);
        assert_eq!(all.len(), 3, "hidden folders are skipped");
        assert_eq!(name_for("~/code/billing-app/"), "billing-app");
    }
}
