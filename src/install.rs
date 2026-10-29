//! Self-installer (Windows): copies this executable to `%ProgramFiles%\ptar`
//! and adds that directory to the system `PATH`.
//!
//! Triggered by running `ptar.exe` with no arguments (e.g. double-click),
//! or explicitly with `--install` / `--uninstall`.

use std::{
    borrow::Cow,
    env, fs, io,
    path::{Path, PathBuf},
    process::Command,
};

use winreg::{
    RegKey, RegValue,
    enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WRITE, RegType},
};

const ENV_KEY: &str = r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment";
const ELEVATED_FLAG: &str = "--elevated";

/// Handle installer arguments. Returns `None` if the args are a normal CLI invocation.
pub fn try_run(args: &[String]) -> Option<io::Result<()>> {
    let rest = &args[1..];
    let elevated = rest.iter().any(|a| a == ELEVATED_FLAG);
    let rest: Vec<&str> = rest
        .iter()
        .map(String::as_str)
        .filter(|a| *a != ELEVATED_FLAG)
        .collect();

    let (uninstall, interactive) = match rest.as_slice() {
        [] => (false, true), // double-click: install and keep the window open
        ["--install"] => (false, elevated),
        ["--uninstall"] => (true, elevated),
        _ => return None,
    };

    let result = if uninstall {
        uninstall_impl(elevated)
    } else {
        install_impl(elevated)
    };
    if let Err(e) = &result {
        eprintln!("ptar: error: {e}");
    }
    if interactive {
        eprintln!("Press Enter to close...");
        let _ = io::stdin().read_line(&mut String::new());
    }
    if result.is_err() {
        std::process::exit(1); // error already reported above
    }
    Some(result)
}

fn install_dir() -> io::Result<PathBuf> {
    let pf = env::var_os("ProgramW6432")
        .or_else(|| env::var_os("ProgramFiles"))
        .ok_or_else(|| io::Error::other("%ProgramFiles% is not set"))?;
    Ok(PathBuf::from(pf).join("ptar"))
}

fn install_impl(elevated: bool) -> io::Result<()> {
    let dir = install_dir()?;
    let dest = dir.join("ptar.exe");
    let src = env::current_exe()?;

    let copied = fs::create_dir_all(&dir).and_then(|_| {
        // Running the already-installed copy: nothing to copy.
        if same_file(&src, &dest) {
            Ok(())
        } else {
            fs::copy(&src, &dest).map(|_| ())
        }
    });
    match copied {
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied && !elevated => {
            return relaunch_elevated("--install");
        }
        other => other?,
    }

    let added = add_to_path(&dir)?;
    println!("Installed {}", dest.display());
    if added {
        println!("Added {} to the system PATH.", dir.display());
    } else {
        println!("{} is already on the system PATH.", dir.display());
    }
    println!("Open a new terminal to use `ptar`.");
    Ok(())
}

fn uninstall_impl(elevated: bool) -> io::Result<()> {
    let dir = install_dir()?;
    match remove_from_path(&dir) {
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied && !elevated => {
            return relaunch_elevated("--uninstall");
        }
        other => other?,
    }
    // The running exe can't delete itself on Windows; remove the directory if
    // we're not running from it, otherwise leave the file in place.
    let running_from_dir = env::current_exe().is_ok_and(|e| e.starts_with(&dir));
    if running_from_dir {
        println!("Removed {} from PATH. Delete the folder manually.", dir.display());
    } else {
        let _ = fs::remove_dir_all(&dir);
        println!("Removed {} and its PATH entry.", dir.display());
    }
    Ok(())
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Re-run this executable through UAC and wait for it to finish.
fn relaunch_elevated(arg: &str) -> io::Result<()> {
    let exe = env::current_exe()?;
    let status = Command::new("powershell")
        .args(["-NoProfile", "-Command"])
        .arg(format!(
            "Start-Process -FilePath '{}' -ArgumentList '{arg} {ELEVATED_FLAG}' -Verb RunAs -Wait",
            exe.display().to_string().replace('\'', "''")
        ))
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "administrator elevation was declined or failed",
        ))
    }
}

fn read_path(key: &RegKey) -> io::Result<String> {
    match key.get_value::<String, _>("Path") {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e),
    }
}

fn write_path(key: &RegKey, path: &str) -> io::Result<()> {
    // Keep REG_EXPAND_SZ so existing %VAR% entries keep working.
    let mut bytes: Vec<u8> = path
        .encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect();
    bytes.shrink_to_fit();
    key.set_raw_value(
        "Path",
        &RegValue {
            bytes: Cow::Owned(bytes),
            vtype: RegType::REG_EXPAND_SZ,
        },
    )?;
    broadcast_env_change();
    Ok(())
}

fn same_dir(entry: &str, dir: &str) -> bool {
    entry
        .trim()
        .trim_end_matches('\\')
        .eq_ignore_ascii_case(dir.trim_end_matches('\\'))
}

/// Returns `true` if the directory was added, `false` if it was already present.
fn add_to_path(dir: &Path) -> io::Result<bool> {
    let dir = dir.to_string_lossy();
    let key = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(ENV_KEY, KEY_READ | KEY_WRITE)?;
    let current = read_path(&key)?;
    if current.split(';').any(|e| same_dir(e, &dir)) {
        return Ok(false);
    }
    let new = if current.is_empty() || current.ends_with(';') {
        format!("{current}{dir}")
    } else {
        format!("{current};{dir}")
    };
    write_path(&key, &new)?;
    Ok(true)
}

fn remove_from_path(dir: &Path) -> io::Result<()> {
    let dir = dir.to_string_lossy();
    let key = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(ENV_KEY, KEY_READ | KEY_WRITE)?;
    let current = read_path(&key)?;
    let kept: Vec<&str> = current
        .split(';')
        .filter(|e| !e.is_empty() && !same_dir(e, &dir))
        .collect();
    write_path(&key, &kept.join(";"))
}

/// Tell running programs (Explorer, new terminals) that the environment changed.
fn broadcast_env_change() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
    };
    let param: Vec<u16> = "Environment\0".encode_utf16().collect();
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            param.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            5000,
            std::ptr::null_mut(),
        );
    }
}
