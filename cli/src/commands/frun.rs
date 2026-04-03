use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

fn cache_path() -> std::path::PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap().join(".cache"))
        .join("dotcli")
        .join("frun.toml")
}

fn load_cache() -> HashMap<String, String> {
    let path = cache_path();
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_cache(emu: &str, flavor: &str) {
    let path = cache_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut map = HashMap::new();
    map.insert("emulator".to_string(), emu.to_string());
    map.insert("flavor".to_string(), flavor.to_string());
    let _ = fs::write(&path, toml::to_string(&map).unwrap_or_default());
}

pub fn run() {
    let adb = format!(
        r"{}\Android\Sdk\platform-tools\adb.exe",
        std::env::var("LOCALAPPDATA").unwrap_or_default()
    );
    let flavors = ["develop", "staging", "production"];
    let cache = load_cache();

    // Get emulators
    let emu_output = Command::new("flutter.bat")
        .args(["emulators"])
        .stderr(Stdio::null())
        .output();

    let emu_list = match emu_output {
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stdout);
            text.lines()
                .filter(|l| l.contains('•'))
                .skip(1)
                .map(|l| l.split('•').next().unwrap_or("").trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        }
        Err(e) => {
            eprintln!("Failed to run flutter emulators: {e}");
            return;
        }
    };

    if emu_list.is_empty() {
        eprintln!("No emulators found");
        return;
    }

    // fzf: emulator selection (previous selection as default)
    let prev_emu = cache.get("emulator").map(|s| s.as_str());
    let prompt_emu = match prev_emu {
        Some(e) => format!("Emulator [{e}]> "),
        None => "Emulator> ".into(),
    };
    let Some(emu) = fzf_select(&emu_list, &prompt_emu, prev_emu) else {
        return;
    };

    // fzf: flavor selection (previous selection as default)
    let flavor_list: Vec<String> = flavors.iter().map(|s| s.to_string()).collect();
    let prev_flavor = cache.get("flavor").map(|s| s.as_str());
    let prompt_flavor = match prev_flavor {
        Some(f) => format!("Flavor [{f}]> "),
        None => "Flavor> ".into(),
    };
    let Some(flavor) = fzf_select(&flavor_list, &prompt_flavor, prev_flavor) else {
        return;
    };

    // Save selections
    save_cache(&emu, &flavor);
    eprintln!("{emu} / {flavor}");

    // Count emulators before launch
    let before = count_emulators(&adb);

    // Launch emulator
    let _ = Command::new("flutter.bat")
        .args(["emulators", "--launch", &emu])
        .status();

    // Wait for device if new emulator started
    if count_emulators(&adb) > before {
        let _ = Command::new(&adb).args(["wait-for-device"]).status();
        let _ = Command::new(&adb)
            .args([
                "shell",
                "while [[ -z $(getprop sys.boot_completed) ]]; do sleep 1; done",
            ])
            .status();
    }

    // Run flutter
    let _ = Command::new("flutter.bat")
        .args(["run", "--flavor", &flavor])
        .status();
}

fn fzf_select(items: &[String], prompt: &str, default: Option<&str>) -> Option<String> {
    const PREV_SUFFIX: &str = " (prev)";

    // Move previous selection to top and mark it
    let ordered: Vec<String> = if let Some(d) = default {
        let mut v = vec![];
        if let Some(item) = items.iter().find(|i| i.as_str() == d) {
            v.push(format!("{item}{PREV_SUFFIX}"));
        }
        v.extend(items.iter().filter(|i| i.as_str() != d).cloned());
        v
    } else {
        items.to_vec()
    };

    let mut fzf = Command::new("fzf")
        .args([&format!("--prompt={prompt}"), "--no-sort", "--pointer=▶"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to start fzf");

    if let Some(mut stdin) = fzf.stdin.take() {
        let _ = stdin.write_all(ordered.join("\n").as_bytes());
    }

    let output = fzf.wait_with_output().expect("fzf wait");
    if output.status.success() {
        let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Some(raw.strip_suffix(PREV_SUFFIX).unwrap_or(&raw).to_string())
    } else {
        None
    }
}

fn count_emulators(adb: &str) -> usize {
    Command::new(adb)
        .args(["devices"])
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|l| l.contains("emulator"))
                .count()
        })
        .unwrap_or(0)
}
