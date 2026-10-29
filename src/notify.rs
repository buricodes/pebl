use crate::app::{Channel, Outgoing};
use crate::backend::tmux::Tmux;
use crate::config::Notify;
use crate::paths;
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub fn deliver(batch: Vec<Outgoing>, cfg: &Notify) {
    let mut looking: Option<bool> = None;
    for n in batch {
        match n.channel {
            Channel::Chime => chime(&n.title),
            Channel::Desktop | Channel::Phone => {
                let front = *looking.get_or_insert_with(|| {
                    let ttys = Tmux::new(paths::tmux_socket()).client_ttys();
                    !ttys.is_empty() && crate::hotkey::pebl_in_front(&ttys)
                });
                if front {
                    continue;
                }
                if n.channel == Channel::Phone {
                    phone(&cfg.phone, &n);
                } else {
                    desktop(&n);
                }
            }
        }
    }
}

pub fn sound_path(name: &str) -> PathBuf {
    if name.contains('/') {
        crate::model::untilde(name)
    } else {
        PathBuf::from(format!("/System/Library/Sounds/{name}.aiff"))
    }
}

pub fn chime(name: &str) {
    let path = sound_path(name);
    let _ = Command::new("afplay")
        .arg(&path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn desktop(n: &Outgoing) {
    let script =
        "on run argv\ndisplay notification (item 2 of argv) with title (item 1 of argv)\nend run";
    let _ = Command::new("osascript")
        .args(["-e", script, &n.title, &n.body])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn phone(topic: &str, n: &Outgoing) {
    let url = if topic.contains("://") {
        topic.to_string()
    } else {
        format!("https://ntfy.sh/{topic}")
    };
    let _ = Command::new("curl")
        .args(["-s", "-m", "10", "-H"])
        .arg(format!("Title: {}", n.title))
        .args(["-d", &n.body, &url])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}
