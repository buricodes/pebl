pub mod hook;

use crate::model::Status;
use serde_json::Value;

pub fn from_hook(event: &str, payload: &Value, current: Status) -> Option<Status> {
    match event {
        "SessionStart" => Some(Status::Idle),
        "UserPromptSubmit" | "PreToolUse" | "PostToolUse" => Some(Status::Running),
        "PermissionRequest" => Some(Status::Waiting),
        "Notification" => notification(payload, current),
        "Stop" => Some(Status::Idle),
        "StopFailure" => Some(Status::Error),
        "SessionEnd" => Some(Status::Exited),
        _ => None,
    }
}

fn notification(payload: &Value, current: Status) -> Option<Status> {
    let kind = payload
        .get("notification_type")
        .and_then(Value::as_str)
        .unwrap_or("");
    match kind {
        "permission_prompt" | "elicitation_dialog" => return Some(Status::Waiting),
        "idle_prompt" | "auth_success" => return None,
        _ => {}
    }
    let msg = payload
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_lowercase();
    if msg.contains("permission") || msg.contains("needs your") || msg.contains("approval") {
        Some(Status::Waiting)
    } else if msg.contains("waiting for your input") {
        (current == Status::Running).then_some(Status::Idle)
    } else {
        None
    }
}

pub fn from_screen(text: &str) -> Status {
    let tail: Vec<&str> = text
        .lines()
        .rev()
        .filter(|l| !l.trim().is_empty())
        .take(25)
        .collect();
    let has = |needle: &str| tail.iter().any(|l| l.contains(needle));
    let spinner = tail.iter().any(|l| {
        let l = l.trim_start();
        let glyph = l
            .chars()
            .next()
            .is_some_and(|c| !c.is_alphanumeric() && !c.is_whitespace());
        glyph
            && l.contains("… (")
            && (l.contains("tokens") || l.contains("s ·") || l.contains("thinking"))
    });
    if has("Do you want to") || has("❯ 1. Yes") || has("trust this folder") {
        Status::Waiting
    } else if has("esc to interrupt") || spinner {
        Status::Running
    } else {
        Status::Idle
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn permission_notification_waits() {
        let p = json!({"notification_type": "permission_prompt", "message": "Claude needs your permission to use Bash"});
        assert_eq!(
            from_hook("Notification", &p, Status::Running),
            Some(Status::Waiting)
        );
    }

    #[test]
    fn idle_reminder_is_not_waiting() {
        let p = json!({"notification_type": "idle_prompt", "message": "Claude is waiting for your input"});
        assert_eq!(from_hook("Notification", &p, Status::Idle), None);
        let legacy = json!({"message": "Claude is waiting for your input"});
        assert_eq!(from_hook("Notification", &legacy, Status::Idle), None);
    }

    #[test]
    fn lifecycle() {
        let n = Value::Null;
        assert_eq!(
            from_hook("UserPromptSubmit", &n, Status::Idle),
            Some(Status::Running)
        );
        assert_eq!(from_hook("Stop", &n, Status::Running), Some(Status::Idle));
        assert_eq!(
            from_hook("StopFailure", &n, Status::Running),
            Some(Status::Error)
        );
        assert_eq!(
            from_hook("SessionEnd", &n, Status::Idle),
            Some(Status::Exited)
        );
        assert_eq!(from_hook("SubagentStop", &n, Status::Running), None);
    }

    #[test]
    fn screen_heuristic() {
        assert_eq!(
            from_screen("✻ Refactoring… (47s · esc to interrupt)\n"),
            Status::Running
        );
        assert_eq!(
            from_screen("Do you want to proceed?\n❯ 1. Yes\n"),
            Status::Waiting
        );
        assert_eq!(from_screen("> \n? for shortcuts\n"), Status::Idle);
        assert_eq!(
            from_screen("✻ Boogieing… (47s · ↓ 687 tokens · thinking some more)\n  ⎿  Tip: Running multiple Claude sessions?\n❯ \n"),
            Status::Running
        );
        assert_eq!(
            from_screen("✻ Churned for 3s · done 10:59 PM\n❯ \n"),
            Status::Idle
        );
        assert_eq!(
            from_screen("Is this a project you created or one you trust?\n❯ No, exit\n  Yes, I trust this folder\n"),
            Status::Waiting
        );
    }
}
