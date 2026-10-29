use crate::backend::tmux::{sh_quote, Tmux};
use crate::paths;
use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use std::process::Command;

const LABEL: &str = "com.pebl.hotkey";

const SWIFT: &str = r#"
import Cocoa
import Carbon

let args = CommandLine.arguments
let trigger = args.count > 1 ? args[1] : "cmd"
let command = args.count > 2 ? args[2] : "pebl summon"
let window = 0.35

var lastTap = 0.0
var modDown = false
var dirty = false
var tap: CFMachPort?

let masks: [String: CGEventFlags] = [
    "ctrl": .maskControl, "option": .maskAlternate, "cmd": .maskCommand, "shift": .maskShift,
]
let allMods: CGEventFlags = [.maskControl, .maskAlternate, .maskCommand, .maskShift]

func fire() {
    let p = Process()
    p.executableURL = URL(fileURLWithPath: "/bin/sh")
    p.arguments = ["-c", command]
    try? p.run()
}

func tapped(_ now: Double) {
    if now - lastTap < window {
        lastTap = 0
        fire()
    } else {
        lastTap = now
    }
}

let callback: CGEventTapCallBack = { _, type, event, _ in
    if type == .tapDisabledByTimeout || type == .tapDisabledByUserInput {
        if let t = tap { CGEvent.tapEnable(tap: t, enable: true) }
        return Unmanaged.passUnretained(event)
    }
    let now = ProcessInfo.processInfo.systemUptime
    if trigger == "space" {
        if type == .keyDown {
            let code = event.getIntegerValueField(.keyboardEventKeycode)
            let rep = event.getIntegerValueField(.keyboardEventAutorepeat)
            if code == 49 && rep == 0 && event.flags.intersection(allMods).isEmpty {
                tapped(now)
            } else {
                lastTap = 0
            }
        }
        return Unmanaged.passUnretained(event)
    }
    guard let mask = masks[trigger] else { return Unmanaged.passUnretained(event) }
    if type == .keyDown {
        dirty = true
        lastTap = 0
    } else if type == .flagsChanged {
        let down = event.flags.contains(mask)
        let others = event.flags.intersection(allMods).subtracting(mask)
        if down && !modDown {
            modDown = true
            dirty = !others.isEmpty
        } else if !down && modDown {
            modDown = false
            if dirty { lastTap = 0 } else { tapped(now) }
        }
    }
    return Unmanaged.passUnretained(event)
}

func log(_ s: String) {
    FileHandle.standardError.write("pebl-hotkey: \(s)\n".data(using: .utf8)!)
}

let keyCodes: [String: Int] = [
    "a": 0, "s": 1, "d": 2, "f": 3, "h": 4, "g": 5, "z": 6, "x": 7, "c": 8, "v": 9, "b": 11,
    "q": 12, "w": 13, "e": 14, "r": 15, "y": 16, "t": 17, "1": 18, "2": 19, "3": 20, "4": 21,
    "6": 22, "5": 23, "9": 25, "7": 26, "8": 28, "0": 29, "o": 31, "u": 32, "i": 34, "p": 35,
    "l": 37, "j": 38, "k": 40, "n": 45, "m": 46, "space": 49, "return": 36, "escape": 53,
]

if trigger.contains("+") {
    var mods = 0
    var code = -1
    for part in trigger.lowercased().split(separator: "+").map(String.init) {
        switch part {
        case "cmd": mods |= cmdKey
        case "shift": mods |= shiftKey
        case "option", "alt": mods |= optionKey
        case "ctrl": mods |= controlKey
        default: code = keyCodes[part] ?? -1
        }
    }
    if code < 0 || mods == 0 {
        log("bad combo \(trigger)")
        exit(1)
    }
    var ref: EventHotKeyRef?
    let id = EventHotKeyID(signature: OSType(0x7065_626c), id: 1)
    let status = RegisterEventHotKey(UInt32(code), UInt32(mods), id, GetApplicationEventTarget(), 0, &ref)
    if status != noErr {
        log("could not register \(trigger) (already used by another app?)")
        exit(1)
    }
    var spec = EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
    InstallEventHandler(GetApplicationEventTarget(), { _, _, _ in fire(); return noErr }, 1, &spec, nil, nil)
    log("listening for \(trigger)")
    NSApplication.shared.setActivationPolicy(.prohibited)
    NSApplication.shared.run()
    exit(0)
}

if !CGPreflightListenEventAccess() {
    CGRequestListenEventAccess()
    FileHandle.standardError.write("pebl-hotkey: allow it under System Settings > Privacy & Security > Input Monitoring\n".data(using: .utf8)!)
    while !CGPreflightListenEventAccess() { sleep(3) }
}
log("listening for double-tap \(trigger)")

let mask = (1 << CGEventType.keyDown.rawValue) | (1 << CGEventType.flagsChanged.rawValue)
guard let t = CGEvent.tapCreate(tap: .cgSessionEventTap, place: .headInsertEventTap, options: .listenOnly,
                                eventsOfInterest: CGEventMask(mask), callback: callback, userInfo: nil) else {
    FileHandle.standardError.write("pebl-hotkey: could not create event tap\n".data(using: .utf8)!)
    exit(1)
}
tap = t
let src = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, t, 0)
CFRunLoopAddSource(CFRunLoopGetCurrent(), src, .commonModes)
CGEvent.tapEnable(tap: t, enable: true)
CFRunLoopRun()
"#;

fn valid_trigger(t: &str) -> bool {
    const MODS: [&str; 5] = ["ctrl", "option", "alt", "cmd", "shift"];
    if ["ctrl", "option", "cmd", "shift", "space"].contains(&t) {
        return true;
    }
    let parts: Vec<&str> = t.split('+').collect();
    let keys: Vec<&&str> = parts.iter().filter(|p| !MODS.contains(p)).collect();
    parts.len() >= 2
        && keys.len() == 1
        && (keys[0].len() == 1 && keys[0].chars().all(|c| c.is_ascii_alphanumeric())
            || ["space", "return", "escape"].contains(keys[0]))
}

fn bin_dir() -> PathBuf {
    paths::home().join("bin")
}

fn plist_path() -> Result<PathBuf> {
    Ok(dirs::home_dir()
        .context("no home dir")?
        .join("Library/LaunchAgents")
        .join(format!("{LABEL}.plist")))
}

fn uid() -> String {
    Command::new("id")
        .arg("-u")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

pub fn install(trigger: &str) -> Result<()> {
    if !valid_trigger(trigger) {
        bail!("trigger must be ctrl, option, cmd, shift, space (double-tap) or a combo like cmd+shift+p");
    }
    let dir = bin_dir();
    std::fs::create_dir_all(&dir)?;
    let src = dir.join("pebl-hotkey.swift");
    let bin = dir.join("pebl-hotkey");
    std::fs::write(&src, SWIFT)?;
    let out = Command::new("swiftc")
        .args(["-O", "-o"])
        .arg(&bin)
        .arg(&src)
        .output()
        .context("running swiftc (install Xcode command line tools: xcode-select --install)")?;
    if !out.status.success() {
        bail!("swiftc failed:\n{}", String::from_utf8_lossy(&out.stderr));
    }

    let pebl = crate::self_exe();
    let plist = plist_path()?;
    std::fs::create_dir_all(plist.parent().context("bad plist path")?)?;
    let log = paths::home().join("hotkey.log");
    let esc = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    std::fs::write(
        &plist,
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{}</string>
    <string>{trigger}</string>
    <string>{}</string>
  </array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>ThrottleInterval</key><integer>10</integer>
  <key>StandardErrorPath</key><string>{}</string>
</dict>
</plist>
"#,
            esc(&bin.display().to_string()),
            esc(&format!("{} summon", sh_quote(&pebl))),
            esc(&log.display().to_string()),
        ),
    )?;
    let domain = format!("gui/{}", uid());
    let _ = Command::new("launchctl")
        .args(["bootout", &format!("{domain}/{LABEL}")])
        .output();
    let out = Command::new("launchctl")
        .args(["bootstrap", &domain])
        .arg(&plist)
        .output()?;
    if !out.status.success() {
        bail!(
            "launchctl bootstrap: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

pub fn uninstall() -> Result<()> {
    let domain = format!("gui/{}", uid());
    let _ = Command::new("launchctl")
        .args(["bootout", &format!("{domain}/{LABEL}")])
        .output();
    let plist = plist_path()?;
    if plist.exists() {
        std::fs::remove_file(plist)?;
    }
    Ok(())
}

pub fn status() -> String {
    let domain = format!("gui/{}/{LABEL}", uid());
    let running = Command::new("launchctl")
        .args(["print", &domain])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if running {
        "installed and running".into()
    } else {
        "not installed".into()
    }
}

pub fn summon(terminal: Option<&str>) -> Result<()> {
    let app = terminal
        .map(str::to_string)
        .unwrap_or_else(default_terminal);
    let ttys = Tmux::new(paths::tmux_socket()).client_ttys();
    if app == "Terminal" && !ttys.is_empty() && focus_terminal_tab(&ttys) {
        return Ok(());
    }
    if app == "iTerm" && !ttys.is_empty() && focus_iterm_session(&ttys) {
        return Ok(());
    }
    open_new(&app)
}

fn default_terminal() -> String {
    for (bundle, name) in [("Ghostty.app", "Ghostty"), ("iTerm.app", "iTerm")] {
        if std::env::var("TERM_PROGRAM").ok().as_deref() == Some(&bundle.replace(".app", "")) {
            return name.into();
        }
    }
    "Terminal".into()
}

fn osascript(script: &str) -> bool {
    Command::new("osascript")
        .args(["-e", script])
        .output()
        .map(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).trim() == "ok")
        .unwrap_or(false)
}

fn as_list(ttys: &[String]) -> String {
    let items: Vec<String> = ttys.iter().map(|t| format!("\"{t}\"")).collect();
    format!("{{{}}}", items.join(", "))
}

fn focus_terminal_tab(ttys: &[String]) -> bool {
    osascript(&format!(
        r#"if application "Terminal" is not running then return "no"
tell application "Terminal"
  repeat with w in windows
    repeat with t in tabs of w
      if {} contains (tty of t) then
        set selected of t to true
        set index of w to 1
        activate
        return "ok"
      end if
    end repeat
  end repeat
end tell
return "no""#,
        as_list(ttys)
    ))
}

pub fn pebl_in_front(ttys: &[String]) -> bool {
    let list = as_list(ttys);
    let front = |app: &str, tty: &str| {
        osascript(&format!(
            r#"if application "{app}" is not running then return "no"
tell application "{app}"
  if frontmost and {list} contains ({tty}) then return "ok"
end tell
return "no""#
        ))
    };
    front("Terminal", "tty of selected tab of front window")
        || (std::path::Path::new("/Applications/iTerm.app").exists()
            && front("iTerm", "tty of current session of current window"))
}

fn focus_iterm_session(ttys: &[String]) -> bool {
    osascript(&format!(
        r#"if application "iTerm" is not running then return "no"
tell application "iTerm"
  repeat with w in windows
    repeat with t in tabs of w
      repeat with s in sessions of t
        if {} contains (tty of s) then
          select w
          tell t to select
          activate
          return "ok"
        end if
      end repeat
    end repeat
  end repeat
end tell
return "no""#,
        as_list(ttys)
    ))
}

fn launcher() -> Result<PathBuf> {
    let dir = bin_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("pebl.command");
    std::fs::write(
        &path,
        format!(
            "#!/bin/zsh -il\ncd ~\nexec {}\n",
            sh_quote(&crate::self_exe())
        ),
    )?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

fn open_new(app: &str) -> Result<()> {
    let ok = match app {
        "Ghostty" => Command::new("open")
            .args([
                "-na",
                "Ghostty",
                "--args",
                "-e",
                &format!("/bin/zsh -ilc {}", sh_quote(&crate::self_exe())),
            ])
            .status()?
            .success(),
        _ => Command::new("open")
            .args(["-a", app])
            .arg(launcher()?)
            .status()?
            .success(),
    };
    if !ok {
        bail!("could not open {app}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triggers() {
        assert!(valid_trigger("cmd"));
        assert!(valid_trigger("cmd+shift+p"));
        assert!(valid_trigger("option+space"));
        assert!(!valid_trigger("p"));
        assert!(!valid_trigger("cmd+shift"));
        assert!(!valid_trigger("cmd+p+q"));
    }
}
