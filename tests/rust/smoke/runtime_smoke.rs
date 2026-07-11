//! Process-level smoke tests for the supported persistence topologies.

use std::{
    error::Error,
    fs::File,
    io::{Read as _, Write as _},
    net::{SocketAddr, TcpListener, TcpStream},
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use tempfile::TempDir;

const SESSION_SECRET: &str = "test-session-secret-with-at-least-32-bytes";
const ENCRYPTION_KEY: &str = "test-encryption-key-with-at-least-32-bytes";

#[test]
fn sqlite_server_and_worker_boot_with_isolated_account_databases() -> Result<(), Box<dyn Error>> {
    let directory = TempDir::new()?;
    let management_path = directory.path().join("management.sqlite3");
    let accounts_dir = directory.path().join("accounts");
    std::fs::create_dir_all(&accounts_dir)?;
    File::create(accounts_dir.join("account-alpha.sqlite3"))?;
    File::create(accounts_dir.join("account-beta.sqlite3"))?;

    let settings = [
        ("PVLOG_DATABASE__BACKEND", "sqlite"),
        (
            "PVLOG_DATABASE__SQLITE__MANAGEMENT_PATH",
            path_text(&management_path)?,
        ),
        (
            "PVLOG_DATABASE__SQLITE__ACCOUNTS_DIR",
            path_text(&accounts_dir)?,
        ),
    ];

    run_migrations(&settings)?;
    run_worker(&settings)?;
    run_server(&settings)?;

    assert!(management_path.is_file());
    assert!(std::fs::metadata(accounts_dir.join("account-alpha.sqlite3"))?.len() > 0);
    assert!(std::fs::metadata(accounts_dir.join("account-beta.sqlite3"))?.len() > 0);
    Ok(())
}

#[test]
fn postgres_server_and_worker_boot_when_test_database_is_available() -> Result<(), Box<dyn Error>> {
    let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
        return Ok(());
    };
    let settings = [
        ("PVLOG_DATABASE__BACKEND", "postgres"),
        ("PVLOG_DATABASE__POSTGRES__URL", url.as_str()),
    ];

    run_migrations(&settings)?;
    run_worker(&settings)?;
    run_server(&settings)?;
    Ok(())
}

#[test]
fn dotenv_and_toml_configuration_are_loaded_together() -> Result<(), Box<dyn Error>> {
    let directory = TempDir::new()?;
    let management_path = directory.path().join("from-dotenv.sqlite3");
    let accounts_dir = directory.path().join("from-toml-accounts");
    std::fs::create_dir_all(&accounts_dir)?;
    std::fs::write(
        directory.path().join(".env"),
        format!(
            "PVLOG_ENVIRONMENT=test\n\
             PVLOG_SECURITY__SESSION_SECRET={SESSION_SECRET}\n\
             PVLOG_SECURITY__CREDENTIAL_ENCRYPTION_KEY={ENCRYPTION_KEY}\n\
             PVLOG_DATABASE__SQLITE__MANAGEMENT_PATH={}\n",
            management_path.display()
        ),
    )?;
    std::fs::write(
        directory.path().join("pvlog.toml"),
        format!(
            "[database]\nbackend = \"sqlite\"\n\
             [database.sqlite]\naccounts_dir = \"{}\"\n",
            toml_path(&accounts_dir)
        ),
    )?;

    let status = Command::new(env!("CARGO_BIN_EXE_pvlog"))
        .current_dir(directory.path())
        .env_clear()
        .args(["migrate", "apply"])
        .stdout(Stdio::null())
        .status()?;

    assert!(status.success());
    assert!(management_path.is_file());
    Ok(())
}

fn run_migrations(settings: &[(&str, &str)]) -> Result<(), Box<dyn Error>> {
    for action in ["plan", "apply", "status"] {
        let status = configured_command(settings)
            .args(["migrate", "--json", action])
            .stdout(Stdio::null())
            .status()?;
        if !status.success() {
            return Err(format!("migration {action} exited with {status}").into());
        }
    }
    Ok(())
}

fn run_worker(settings: &[(&str, &str)]) -> Result<(), Box<dyn Error>> {
    let status = configured_command(settings)
        .args(["worker", "--once"])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("worker exited with {status}").into())
    }
}

fn run_server(settings: &[(&str, &str)]) -> Result<(), Box<dyn Error>> {
    let address = available_address()?;
    let mut child = configured_command(settings)
        .env("PVLOG_HTTP__BIND", address.to_string())
        .arg("server")
        .stdout(Stdio::null())
        .spawn()?;

    let result = wait_for_liveness(&mut child, address);
    let _kill_result = child.kill();
    let _wait_result = child.wait();
    result
}

fn configured_command(settings: &[(&str, &str)]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pvlog"));
    command
        .env("PVLOG_ENVIRONMENT", "test")
        .env("PVLOG_SECURITY__SESSION_SECRET", SESSION_SECRET)
        .env("PVLOG_SECURITY__CREDENTIAL_ENCRYPTION_KEY", ENCRYPTION_KEY)
        .env_remove("PVLOG_DATABASE__POSTGRES__URL")
        .envs(settings.iter().copied());
    command
}

fn wait_for_liveness(child: &mut Child, address: SocketAddr) -> Result<(), Box<dyn Error>> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(status) = child.try_wait()? {
            return Err(format!("server exited before becoming ready: {status}").into());
        }
        if let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_millis(200)) {
            stream.write_all(
                b"GET /api/v1/health/live HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
            )?;
            let mut response = String::new();
            stream.read_to_string(&mut response)?;
            if response.starts_with("HTTP/1.1 200") && response.contains("\"status\":\"ok\"") {
                return Ok(());
            }
        }
        if Instant::now() >= deadline {
            return Err("server liveness endpoint did not become ready within 15 seconds".into());
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn available_address() -> Result<SocketAddr, Box<dyn Error>> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    Ok(listener.local_addr()?)
}

fn path_text(path: &Path) -> Result<&str, Box<dyn Error>> {
    path.to_str()
        .ok_or_else(|| format!("test path is not valid UTF-8: {}", path.display()).into())
}

fn toml_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "\\\\")
}
