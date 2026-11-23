//! iOS Simulators and Android emulators on this computer: list them, boot one, install and
//! launch an app the project built, and take a screenshot. Only simulators and emulators are
//! touched, never a phone plugged into the computer: an Android serial must be `emulator-<port>`.
//! Every program is run with fixed arguments and validated values, never through a shell.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;

use crate::{CodeError, fail, io};

const TIMEOUT: Duration = Duration::from_secs(20);
const MAX_OUTPUT_BYTES: usize = 32 * 1024 * 1024;
const MAX_ID_CHARS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Ios,
    Android,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerState {
    Booted,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Simulator {
    /// What every tool takes: `ios:<udid>`, `android:<serial>` of a running emulator or
    /// `android-avd:<name>` of one that is not running.
    pub id: String,
    pub platform: Platform,
    pub name: String,
    pub state: PowerState,
    /// The OS version for iOS, empty for Android.
    pub os: String,
}

#[derive(Debug, Default)]
pub struct Output {
    pub success: bool,
    pub stdout: Vec<u8>,
    pub stderr: String,
}

/// Runs a program and waits for it; a trait so the logic is tested without Xcode or the SDK.
pub trait Exec: Send + Sync {
    fn run(&self, program: &Path, args: &[&str]) -> Result<Output, CodeError>;
}

pub struct System;

fn read_limited(mut pipe: impl Read + Send + 'static) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut out = Vec::new();
        let _ = pipe
            .by_ref()
            .take(MAX_OUTPUT_BYTES as u64)
            .read_to_end(&mut out);
        out
    })
}

impl Exec for System {
    fn run(&self, program: &Path, args: &[&str]) -> Result<Output, CodeError> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| io(&program.display().to_string(), e))?;
        let stdout = child.stdout.take().map(read_limited);
        let stderr = child.stderr.take().map(read_limited);
        let deadline = Instant::now() + TIMEOUT;
        let status = loop {
            match child
                .try_wait()
                .map_err(|e| io("waiting for a program", e))?
            {
                Some(status) => break status,
                None if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return fail(format!(
                        "{} did not answer in {TIMEOUT:?}",
                        program.display()
                    ));
                }
                None => std::thread::sleep(Duration::from_millis(20)),
            }
        };
        let take = |h: Option<std::thread::JoinHandle<Vec<u8>>>| {
            h.and_then(|h| h.join().ok()).unwrap_or_default()
        };
        Ok(Output {
            success: status.success(),
            stdout: take(stdout),
            stderr: String::from_utf8_lossy(&take(stderr)).into_owned(),
        })
    }
}

/// Where the programs are. A GUI app does not inherit the shell's `PATH`, so the usual
/// locations are tried as well.
#[derive(Debug, Clone, Default)]
pub struct Programs {
    pub xcrun: Option<PathBuf>,
    pub adb: Option<PathBuf>,
    pub emulator: Option<PathBuf>,
}

fn find(name: &str, extra: &[PathBuf]) -> Option<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .chain(extra.iter().cloned())
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

impl Programs {
    pub fn locate() -> Programs {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let sdk = ["ANDROID_HOME", "ANDROID_SDK_ROOT"]
            .iter()
            .filter_map(|v| std::env::var_os(v).map(PathBuf::from))
            .chain(home.iter().map(|h| h.join("Library/Android/sdk")))
            .chain(home.iter().map(|h| h.join("Android/Sdk")))
            .find(|dir| dir.is_dir());
        let in_sdk = |sub: &str| sdk.iter().map(|s| s.join(sub)).collect::<Vec<_>>();
        Programs {
            xcrun: cfg!(target_os = "macos")
                .then(|| find("xcrun", &[PathBuf::from("/usr/bin")]))
                .flatten(),
            adb: find("adb", &in_sdk("platform-tools")),
            emulator: find("emulator", &in_sdk("emulator")),
        }
    }
}

pub struct Simulators<E: Exec> {
    exec: E,
    programs: Programs,
}

fn plain_id(what: &str, text: &str, extra: &[char]) -> Result<(), CodeError> {
    let ok = !text.is_empty()
        && text.chars().count() <= MAX_ID_CHARS
        && text.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') || extra.contains(&c)
        });
    if ok {
        Ok(())
    } else {
        fail(format!(
            "{what} {text:?} is not a simulator id from list_simulators"
        ))
    }
}

/// An application id (`com.example.app`): dotted words.
pub fn valid_app_id(id: &str) -> Result<(), CodeError> {
    let ok = !id.is_empty()
        && id.chars().count() <= 128
        && id.split('.').count() >= 2
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if ok {
        Ok(())
    } else {
        fail(format!(
            "{id:?} is not an application id such as com.example.app"
        ))
    }
}

enum Target<'a> {
    Ios(&'a str),
    Android(&'a str),
    Avd(&'a str),
}

fn parse_id(id: &str) -> Result<Target<'_>, CodeError> {
    if let Some(udid) = id.strip_prefix("ios:") {
        plain_id("the iOS simulator", udid, &[])?;
        Ok(Target::Ios(udid))
    } else if let Some(serial) = id.strip_prefix("android:") {
        let port = serial.strip_prefix("emulator-").unwrap_or_default();
        if port.is_empty() || !port.chars().all(|c| c.is_ascii_digit()) {
            return fail(format!(
                "{serial:?} is not an emulator: only emulators are used, never a connected phone"
            ));
        }
        Ok(Target::Android(serial))
    } else if let Some(avd) = id.strip_prefix("android-avd:") {
        plain_id("the Android virtual device", avd, &[])?;
        Ok(Target::Avd(avd))
    } else {
        fail(format!("{id:?} is not a simulator id from list_simulators"))
    }
}

impl<E: Exec> Simulators<E> {
    pub fn new(exec: E, programs: Programs) -> Simulators<E> {
        Simulators { exec, programs }
    }

    fn xcrun(&self) -> Result<&Path, CodeError> {
        self.programs.xcrun.as_deref().ok_or_else(|| {
            CodeError(
                "the iOS Simulator needs Xcode (macOS only): install it from the App Store".into(),
            )
        })
    }

    fn adb(&self) -> Result<&Path, CodeError> {
        self.programs.adb.as_deref().ok_or_else(|| {
            CodeError(
                "adb was not found: install the Android SDK platform-tools, or set ANDROID_HOME"
                    .into(),
            )
        })
    }

    fn run(&self, program: &Path, args: &[&str], what: &str) -> Result<Output, CodeError> {
        let out = self.exec.run(program, args)?;
        if out.success {
            Ok(out)
        } else {
            fail(format!("{what}: {}", out.stderr.trim()))
        }
    }

    pub fn list(&self) -> Result<Vec<Simulator>, CodeError> {
        let mut all = Vec::new();
        if self.programs.xcrun.is_some() {
            all.extend(self.list_ios()?);
        }
        if self.programs.adb.is_some() {
            all.extend(self.list_android());
        }
        Ok(all)
    }

    fn list_ios(&self) -> Result<Vec<Simulator>, CodeError> {
        let out = self.run(
            self.xcrun()?,
            &["simctl", "list", "devices", "available", "-j"],
            "listing iOS simulators",
        )?;
        let json: Value = serde_json::from_slice(&out.stdout)
            .map_err(|e| CodeError(format!("simctl answered something unreadable: {e}")))?;
        Ok(parse_simctl(&json))
    }

    fn list_android(&self) -> Vec<Simulator> {
        let mut found = Vec::new();
        if let Ok(adb) = self.adb()
            && let Ok(out) = self.exec.run(adb, &["devices"])
        {
            found.extend(parse_adb_devices(&String::from_utf8_lossy(&out.stdout)));
        }
        if let Some(emulator) = &self.programs.emulator
            && let Ok(out) = self.exec.run(emulator, &["-list-avds"])
        {
            for name in String::from_utf8_lossy(&out.stdout).lines().map(str::trim) {
                if plain_id("avd", name, &[]).is_ok() {
                    found.push(Simulator {
                        id: format!("android-avd:{name}"),
                        platform: Platform::Android,
                        name: name.to_string(),
                        state: PowerState::Shutdown,
                        os: String::new(),
                    });
                }
            }
        }
        found
    }

    /// Boots an iOS simulator and opens the Simulator app; returns the id to use afterwards.
    /// An Android virtual device is not booted here: [`Simulators::emulator_command`] gives the
    /// command to run in a tmux session, because the emulator keeps running.
    pub fn boot_ios(&self, id: &str) -> Result<(), CodeError> {
        let Target::Ios(udid) = parse_id(id)? else {
            return fail("only an iOS simulator is booted here");
        };
        let xcrun = self.xcrun()?;
        let out = self.exec.run(xcrun, &["simctl", "boot", udid])?;
        let already = out.stderr.contains("current state: Booted");
        if !out.success && !already {
            return fail(format!("booting the simulator: {}", out.stderr.trim()));
        }
        let _ = self
            .exec
            .run(Path::new("/usr/bin/open"), &["-a", "Simulator"]);
        Ok(())
    }

    /// The command line that boots a virtual device, to run in a tmux session.
    pub fn emulator_command(&self, id: &str) -> Result<String, CodeError> {
        let Target::Avd(avd) = parse_id(id)? else {
            return fail("only an Android virtual device that is not running is started this way");
        };
        let emulator = self.programs.emulator.as_deref().ok_or_else(|| {
            CodeError(
                "the Android emulator was not found: install it with Android Studio's SDK Manager"
                    .into(),
            )
        })?;
        Ok(format!(
            "exec '{}' -avd {avd} -no-snapshot-save",
            emulator.display().to_string().replace('\'', "'\\''")
        ))
    }

    pub fn install(&self, id: &str, app: &Path) -> Result<(), CodeError> {
        let path = app.display().to_string();
        match parse_id(id)? {
            Target::Ios(udid) => {
                if app.extension().is_none_or(|e| e != "app") || !app.is_dir() {
                    return fail("an iOS app to install is a .app folder built for the simulator");
                }
                self.run(
                    self.xcrun()?,
                    &["simctl", "install", udid, &path],
                    "installing the app",
                )?;
            }
            Target::Android(serial) => {
                if app.extension().is_none_or(|e| e != "apk") || !app.is_file() {
                    return fail("an Android app to install is an .apk file");
                }
                self.run(
                    self.adb()?,
                    &["-s", serial, "install", "-r", &path],
                    "installing the app",
                )?;
            }
            Target::Avd(_) => return fail("start the virtual device first"),
        }
        Ok(())
    }

    pub fn launch(&self, id: &str, app_id: &str) -> Result<(), CodeError> {
        valid_app_id(app_id)?;
        match parse_id(id)? {
            Target::Ios(udid) => {
                self.run(
                    self.xcrun()?,
                    &["simctl", "launch", udid, app_id],
                    "launching the app",
                )?;
            }
            Target::Android(serial) => {
                self.run(
                    self.adb()?,
                    &[
                        "-s",
                        serial,
                        "shell",
                        "monkey",
                        "-p",
                        app_id,
                        "-c",
                        "android.intent.category.LAUNCHER",
                        "1",
                    ],
                    "launching the app",
                )?;
            }
            Target::Avd(_) => return fail("start the virtual device first"),
        }
        Ok(())
    }

    /// A PNG of the simulator's screen.
    pub fn screenshot(&self, id: &str) -> Result<Vec<u8>, CodeError> {
        let png = match parse_id(id)? {
            Target::Ios(udid) => {
                let out = self.run(
                    self.xcrun()?,
                    &["simctl", "io", udid, "screenshot", "--type=png", "-"],
                    "taking a screenshot",
                )?;
                out.stdout
            }
            Target::Android(serial) => {
                self.run(
                    self.adb()?,
                    &["-s", serial, "exec-out", "screencap", "-p"],
                    "taking a screenshot",
                )?
                .stdout
            }
            Target::Avd(_) => return fail("start the virtual device first"),
        };
        if png.starts_with(b"\x89PNG\r\n\x1a\n") {
            Ok(png)
        } else {
            fail("the screenshot was not a PNG: is the simulator booted?")
        }
    }
}

fn parse_simctl(json: &Value) -> Vec<Simulator> {
    let mut out = Vec::new();
    let Some(runtimes) = json.get("devices").and_then(Value::as_object) else {
        return out;
    };
    for (runtime, devices) in runtimes {
        let os = runtime
            .rsplit('.')
            .next()
            .map(|r| r.replace('-', "."))
            .and_then(|r| r.strip_prefix("iOS.").map(str::to_string))
            .unwrap_or_default();
        if os.is_empty() {
            continue;
        }
        for device in devices.as_array().into_iter().flatten() {
            let (Some(udid), Some(name)) = (
                device.get("udid").and_then(Value::as_str),
                device.get("name").and_then(Value::as_str),
            ) else {
                continue;
            };
            if plain_id("udid", udid, &[]).is_err() {
                continue;
            }
            let booted = device.get("state").and_then(Value::as_str) == Some("Booted");
            out.push(Simulator {
                id: format!("ios:{udid}"),
                platform: Platform::Ios,
                name: name.chars().filter(|c| !c.is_control()).collect(),
                state: if booted {
                    PowerState::Booted
                } else {
                    PowerState::Shutdown
                },
                os: os.clone(),
            });
        }
    }
    out.sort_by(|a, b| (&b.os, &a.name).cmp(&(&a.os, &b.name)));
    out
}

fn parse_adb_devices(text: &str) -> Vec<Simulator> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let serial = parts.next()?;
            let state = parts.next()?;
            let port = serial.strip_prefix("emulator-")?;
            port.chars().all(|c| c.is_ascii_digit()).then_some(())?;
            Some(Simulator {
                id: format!("android:{serial}"),
                platform: Platform::Android,
                name: serial.to_string(),
                state: if state == "device" {
                    PowerState::Booted
                } else {
                    PowerState::Shutdown
                },
                os: String::new(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct Fake {
        calls: Mutex<Vec<Vec<String>>>,
        replies: Mutex<VecDeque<Output>>,
    }

    impl Exec for &Fake {
        fn run(&self, program: &Path, args: &[&str]) -> Result<Output, CodeError> {
            let mut call = vec![program.display().to_string()];
            call.extend(args.iter().map(|a| (*a).to_string()));
            self.calls.lock().unwrap().push(call);
            Ok(self.replies.lock().unwrap().pop_front().unwrap_or(Output {
                success: true,
                ..Output::default()
            }))
        }
    }

    fn programs() -> Programs {
        Programs {
            xcrun: Some("/usr/bin/xcrun".into()),
            adb: Some("/sdk/adb".into()),
            emulator: Some("/sdk/emulator".into()),
        }
    }

    fn reply(stdout: &str) -> Output {
        Output {
            success: true,
            stdout: stdout.as_bytes().to_vec(),
            stderr: String::new(),
        }
    }

    #[test]
    fn simctl_json_becomes_ios_simulators_newest_os_first() {
        let json = serde_json::json!({"devices": {
            "com.apple.CoreSimulator.SimRuntime.iOS-17-5": [
                {"udid": "AAAA-1111", "name": "iPhone 15", "state": "Shutdown"}],
            "com.apple.CoreSimulator.SimRuntime.iOS-18-0": [
                {"udid": "BBBB-2222", "name": "iPhone 16", "state": "Booted"},
                {"udid": "bad id; rm", "name": "x", "state": "Shutdown"}],
            "com.apple.CoreSimulator.SimRuntime.watchOS-11-0": [
                {"udid": "CCCC", "name": "Watch", "state": "Shutdown"}]
        }});
        let found = parse_simctl(&json);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].id, "ios:BBBB-2222");
        assert_eq!(found[0].os, "18.0");
        assert_eq!(found[0].state, PowerState::Booted);
    }

    #[test]
    fn only_emulators_are_listed_never_a_connected_phone() {
        let text = "List of devices attached\nemulator-5554\tdevice\nR58M123ABC\tdevice\nemulator-5556\toffline\n";
        let found = parse_adb_devices(text);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].id, "android:emulator-5554");
        assert_eq!(found[0].state, PowerState::Booted);
        assert_eq!(found[1].state, PowerState::Shutdown);
    }

    #[test]
    fn ids_are_validated_before_any_program_runs() {
        let fake = Fake::default();
        let sims = Simulators::new(&fake, programs());
        for id in [
            "android:R58M123ABC",
            "ios:a b",
            "ios:",
            "x",
            "android:emulator-",
            "ios:a;b",
        ] {
            assert!(sims.screenshot(id).is_err(), "{id}");
            assert!(sims.launch(id, "com.example.app").is_err(), "{id}");
        }
        assert!(sims.launch("ios:AAAA", "not an id").is_err());
        assert!(sims.launch("ios:AAAA", "--help.x y").is_err());
        assert!(fake.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn a_screenshot_must_be_a_png() {
        let fake = Fake::default();
        let sims = Simulators::new(&fake, programs());
        fake.replies
            .lock()
            .unwrap()
            .push_back(reply("not a picture"));
        assert!(sims.screenshot("ios:AAAA").is_err());
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend_from_slice(b"rest");
        fake.replies.lock().unwrap().push_back(Output {
            success: true,
            stdout: png.clone(),
            ..Output::default()
        });
        assert_eq!(sims.screenshot("android:emulator-5554").unwrap(), png);
        let calls = fake.calls.lock().unwrap();
        assert_eq!(
            calls[1],
            [
                "/sdk/adb",
                "-s",
                "emulator-5554",
                "exec-out",
                "screencap",
                "-p"
            ]
        );
    }

    #[test]
    fn install_checks_the_kind_of_file() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Fake::default();
        let sims = Simulators::new(&fake, programs());
        let apk = dir.path().join("a.apk");
        std::fs::write(&apk, "x").unwrap();
        assert!(sims.install("android:emulator-5554", &apk).is_ok());
        assert!(sims.install("ios:AAAA", &apk).is_err());
        let app = dir.path().join("A.app");
        std::fs::create_dir(&app).unwrap();
        assert!(sims.install("ios:AAAA", &app).is_ok());
        assert!(sims.install("android:emulator-5554", &app).is_err());
    }

    #[test]
    fn a_missing_sdk_says_what_to_install() {
        let fake = Fake::default();
        let sims = Simulators::new(&fake, Programs::default());
        assert_eq!(sims.list().unwrap().len(), 0);
        assert!(
            sims.screenshot("android:emulator-5554")
                .unwrap_err()
                .0
                .contains("adb")
        );
        assert!(sims.screenshot("ios:AAAA").unwrap_err().0.contains("Xcode"));
    }

    #[test]
    fn a_virtual_device_is_started_by_a_command_for_tmux() {
        let fake = Fake::default();
        let sims = Simulators::new(&fake, programs());
        let command = sims.emulator_command("android-avd:Pixel_8").unwrap();
        assert_eq!(
            command,
            "exec '/sdk/emulator' -avd Pixel_8 -no-snapshot-save"
        );
        assert!(sims.emulator_command("android-avd:a b").is_err());
        assert!(sims.emulator_command("ios:AAAA").is_err());
    }
}
