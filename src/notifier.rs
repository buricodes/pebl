use crate::paths;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const ID: &str = "com.pebl.notify";
const ICON: &[u8] = include_bytes!("../assets/pebl.icns");
const LSREGISTER: &str = "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister";

const PLIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>com.pebl.notify</string>
  <key>CFBundleName</key><string>pebl</string>
  <key>CFBundleDisplayName</key><string>pebl</string>
  <key>CFBundleExecutable</key><string>pebl-notify</string>
  <key>CFBundleIconFile</key><string>pebl</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSUIElement</key><true/>
</dict>
</plist>
"#;

const SWIFT: &str = r#"
import Cocoa
import UserNotifications

let args = CommandLine.arguments
let center = UNUserNotificationCenter.current()

func log(_ s: String) {
    FileHandle.standardError.write("pebl-notify: \(s)\n".data(using: .utf8)!)
}

func run(_ exe: String, _ argv: [String]) {
    let p = Process()
    p.executableURL = URL(fileURLWithPath: exe)
    p.arguments = argv
    try? p.run()
    p.waitUntilExit()
}

class Delegate: NSObject, NSApplicationDelegate, UNUserNotificationCenterDelegate {
    var posting = false

    func applicationWillFinishLaunching(_ n: Notification) {
        center.delegate = self
    }

    func applicationDidFinishLaunching(_ n: Notification) {
        if args.count >= 2 && args[1] == "status" {
            center.getNotificationSettings { s in
                switch s.authorizationStatus {
                case .authorized, .provisional: print(s.alertSetting == .enabled ? "allowed" : "allowed, banners off")
                case .denied: print("denied")
                default: print("not asked yet")
                }
                exit(0)
            }
            return
        }
        if args.count >= 6 && args[1] == "post" {
            posting = true
            post(title: args[2], body: args[3], sid: args[4], pebl: args[5])
            return
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 10) { NSApp.terminate(nil) }
    }

    func post(title: String, body: String, sid: String, pebl: String) {
        center.requestAuthorization(options: [.alert, .sound]) { ok, err in
            if !ok {
                log("not allowed \(err?.localizedDescription ?? "")")
                exit(2)
            }
            let c = UNMutableNotificationContent()
            c.title = title
            c.body = body
            c.userInfo = ["sid": sid, "pebl": pebl]
            c.threadIdentifier = sid
            center.add(UNNotificationRequest(identifier: sid, content: c, trigger: nil)) { err in
                if let e = err {
                    log("post failed \(e.localizedDescription)")
                    exit(3)
                }
                DispatchQueue.main.asyncAfter(deadline: .now() + 3) { exit(0) }
            }
        }
    }

    func userNotificationCenter(_ c: UNUserNotificationCenter, willPresent n: UNNotification,
                                withCompletionHandler done: @escaping (UNNotificationPresentationOptions) -> Void) {
        done([.banner, .list])
    }

    func userNotificationCenter(_ c: UNUserNotificationCenter, didReceive r: UNNotificationResponse,
                                withCompletionHandler done: @escaping () -> Void) {
        let info = r.notification.request.content.userInfo
        if r.actionIdentifier == UNNotificationDefaultActionIdentifier,
           let sid = info["sid"] as? String, let pebl = info["pebl"] as? String {
            run(pebl, ["ctl", "open", sid])
            run(pebl, ["summon"])
        }
        done()
        if !posting { NSApp.terminate(nil) }
    }
}

let app = NSApplication.shared
let delegate = Delegate()
app.delegate = delegate
app.setActivationPolicy(.prohibited)
app.run()
"#;

pub fn app_path() -> PathBuf {
    paths::home().join("bin").join("pebl-notify.app")
}

fn stamp() -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in SWIFT
        .bytes()
        .chain(PLIST.bytes())
        .chain(ICON.iter().copied())
    {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

fn stamp_file(app: &Path) -> PathBuf {
    app.join("Contents").join("Resources").join("stamp")
}

pub fn ready() -> bool {
    std::fs::read_to_string(stamp_file(&app_path())).is_ok_and(|s| s == stamp())
}

pub fn ensure() -> Result<()> {
    if !cfg!(target_os = "macos") || ready() {
        return Ok(());
    }
    let app = app_path();
    let tmp = app.with_extension("building");
    let _ = std::fs::remove_dir_all(&tmp);
    build(&tmp)?;
    let _ = std::fs::remove_dir_all(&app);
    std::fs::rename(&tmp, &app)?;
    let _ = Command::new(LSREGISTER)
        .arg("-f")
        .arg(&app)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    Ok(())
}

fn build(app: &Path) -> Result<()> {
    let contents = app.join("Contents");
    let macos = contents.join("MacOS");
    let res = contents.join("Resources");
    std::fs::create_dir_all(&macos)?;
    std::fs::create_dir_all(&res)?;
    std::fs::write(contents.join("Info.plist"), PLIST)?;
    std::fs::write(res.join("pebl.icns"), ICON)?;
    let src = app
        .parent()
        .context("bad app path")?
        .join("pebl-notify.swift");
    std::fs::write(&src, SWIFT)?;
    let out = Command::new("swiftc")
        .args(["-O", "-o"])
        .arg(macos.join("pebl-notify"))
        .arg(&src)
        .output()
        .context("running swiftc (install Xcode command line tools: xcode-select --install)")?;
    let _ = std::fs::remove_file(&src);
    if !out.status.success() {
        bail!("swiftc failed:\n{}", String::from_utf8_lossy(&out.stderr));
    }
    let out = Command::new("codesign")
        .args(["--force", "--sign", "-"])
        .arg(app)
        .output()?;
    if !out.status.success() {
        bail!("codesign: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    std::fs::write(stamp_file(app), stamp())?;
    Ok(())
}

fn launch(args: &[&str]) -> Option<(String, String)> {
    if !ready() {
        return None;
    }
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let out = paths::home().join(format!("notify-{n}.out"));
    let err = paths::home().join(format!("notify-{n}.err"));
    let ok = Command::new("open")
        .args(["-n", "-W", "--stdout"])
        .arg(&out)
        .arg("--stderr")
        .arg(&err)
        .arg(app_path())
        .arg("--args")
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    let read = |p: &Path| {
        let s = std::fs::read_to_string(p).unwrap_or_default();
        let _ = std::fs::remove_file(p);
        s.trim().to_string()
    };
    let (o, e) = (read(&out), read(&err));
    ok.then_some((o, e))
}

pub fn post(title: &str, body: &str, sid: &str, pebl: &str) -> bool {
    launch(&["post", title, body, sid, pebl]).is_some_and(|(_, err)| err.is_empty())
}

pub fn status() -> String {
    if !ready() {
        return "plain (pebl builds its notifier on the next launch; needs Xcode command line tools)".into();
    }
    match launch(&["status"]) {
        Some((s, _)) if s == "denied" => format!(
            "turned off for pebl: allow it in System Settings > Notifications > pebl ({ID})"
        ),
        Some((s, _)) if !s.is_empty() => format!("{s}; clicking one opens its session"),
        _ => "notifier installed".into(),
    }
}
