use crate::paths;
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};

pub const FILE: &str = "pebl.toml";

pub fn slug(project: &Path) -> String {
    project
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

pub fn private_file(project: &Path) -> PathBuf {
    paths::home()
        .join("projects")
        .join(format!("{}.toml", slug(project)))
}

fn candidates(dirs: &[&Path]) -> Vec<PathBuf> {
    dirs.iter()
        .flat_map(|d| [d.join(FILE), private_file(d)])
        .collect()
}

pub fn section<T: DeserializeOwned>(dirs: &[&Path], name: &str) -> Option<(T, PathBuf)> {
    candidates(dirs).into_iter().find_map(|file| {
        let text = std::fs::read_to_string(&file).ok()?;
        let table: toml::Table = toml::from_str(&text).ok()?;
        let value = table.get(name)?.clone();
        Some((value.try_into().ok()?, file))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize, Debug, PartialEq)]
    struct Queue {
        max_parallel: usize,
    }

    #[test]
    fn reads_a_section_from_the_first_file_that_has_it() {
        let tmp = tempfile::tempdir().unwrap();
        let (wt, repo) = (tmp.path().join("wt"), tmp.path().join("repo"));
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(wt.join(FILE), "[worktree]\ncopy = []\n").unwrap();
        std::fs::write(repo.join(FILE), "[queue]\nmax_parallel = 3\n").unwrap();
        let (q, from) = section::<Queue>(&[&wt, &repo], "queue").unwrap();
        assert_eq!(q, Queue { max_parallel: 3 });
        assert_eq!(from, repo.join(FILE));
        assert!(section::<Queue>(&[&wt], "queue").is_none());
        std::fs::write(repo.join(FILE), "[queue]\nmax_parallel = \"lots\"\n").unwrap();
        assert!(section::<Queue>(&[&repo], "queue").is_none());
    }

    #[test]
    fn private_settings_live_under_pebl_home() {
        let f = private_file(Path::new("/Users/me/code/lief-care"));
        assert!(f.ends_with("projects/Users-me-code-lief-care.toml"));
    }
}
