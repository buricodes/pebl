use std::collections::BTreeSet;

pub struct Tip {
    pub key: &'static str,
    pub text: &'static str,
    pub feature: &'static str,
}

const fn tip(key: &'static str, text: &'static str, feature: &'static str) -> Tip {
    Tip { key, text, feature }
}

pub const TIPS: [Tip; 18] = [
    tip("ctrl+t", "opens a shell in this chat's folder", "shell"),
    tip(
        "+ split",
        "in the shell bar adds a second terminal",
        "shell-new",
    ),
    tip(
        "alt+r",
        "types Claude's suggested command into the shell",
        "run-next",
    ),
    tip(
        "alt+c",
        "sends your shell's output to Claude",
        "shell-to-claude",
    ),
    tip(
        "ctrl+\\",
        "opens the selected chat in a second column",
        "split",
    ),
    tip("ctrl+g", "jumps to the next chat that needs you", "waiting"),
    tip("ctrl+y", "lists everything waiting on you", "inbox"),
    tip(
        "ctrl+p",
        "saved prompts you can reuse in any chat",
        "prompts",
    ),
    tip("ctrl+x", "marks many chats, one prompt to all", "mark"),
    tip("ctrl+f", "searches every past conversation", "search"),
    tip("w", "new chat in its own git worktree", "key w"),
    tip(
        "q",
        "task queue: each task runs in its own worktree",
        "key q",
    ),
    tip("R", "a second Claude reviews this chat's changes", "key R"),
    tip("H", "every turn of this chat; roll back to any", "key H"),
    tip("N", "a project note every new chat starts with", "key N"),
    tip("e", "opens this project in your editor", "key e"),
    tip("t", "sends failing checks back to Claude", "key t"),
    tip("/compact", "context gauge going red? compact early", ""),
];

pub const EVERY_SECS: u64 = 10;

pub fn tracks(feature: &str) -> bool {
    !feature.is_empty() && TIPS.iter().any(|t| t.feature == feature)
}

pub fn pick(used: &BTreeSet<String>, at: u64) -> (usize, &'static Tip) {
    let seen = |i: &usize| used.contains(TIPS[*i].feature);
    let order: Vec<usize> = (0..TIPS.len())
        .filter(|i| !seen(i))
        .chain((0..TIPS.len()).filter(seen))
        .collect();
    let i = order[(at / EVERY_SECS) as usize % order.len()];
    (i, &TIPS[i])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn favours_features_not_used_yet() {
        let mut used = BTreeSet::new();
        assert_eq!(pick(&used, 0).0, 0);
        assert_eq!(pick(&used, EVERY_SECS).0, 1);
        used.insert("shell".to_string());
        assert_eq!(pick(&used, 0).0, 1);
        assert_eq!(pick(&used, EVERY_SECS * 17).0, 0);
        let all: Vec<usize> = (0..18).map(|n| pick(&used, EVERY_SECS * n).0).collect();
        assert_eq!(all.iter().collect::<BTreeSet<_>>().len(), 18);
        assert!(tracks("key q") && !tracks("") && !tracks("next"));
    }

    #[test]
    fn tips_fit_two_sidebar_lines() {
        for t in &TIPS {
            assert!(t.text.chars().count() <= 52, "{}", t.text);
        }
    }
}
