use std::path::Path;

use anyhow::Result;
use clap::ValueEnum;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ServiceManager {
    Systemd,
    Launchd,
}

pub fn render_service(
    manager: ServiceManager,
    label: &str,
    exec_path: &Path,
    env_path: &Path,
    working_dir: &Path,
) -> Result<String> {
    Ok(match manager {
        ServiceManager::Systemd => render_systemd_service(label, exec_path, env_path, working_dir),
        ServiceManager::Launchd => render_launchd_service(label, exec_path, env_path, working_dir),
    })
}

fn shell_escape_systemd_path(path: &Path) -> String {
    let text = path.display().to_string();
    if text.bytes().any(|byte| byte.is_ascii_whitespace()) {
        format!("\"{}\"", text.replace('"', "\\\""))
    } else {
        text
    }
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn render_systemd_service(
    label: &str,
    exec_path: &Path,
    env_path: &Path,
    working_dir: &Path,
) -> String {
    format!(
        "[Unit]\nDescription=Stonr relay ({label})\nWants=network-online.target\nAfter=network-online.target\n\n[Service]\nType=simple\nWorkingDirectory={working_dir}\nExecStart={exec_path} --env {env_path} serve\nRestart=always\nRestartSec=5\nNoNewPrivileges=yes\nLimitNOFILE=65536\n\n[Install]\nWantedBy=multi-user.target\n",
        label = label,
        working_dir = shell_escape_systemd_path(working_dir),
        exec_path = shell_escape_systemd_path(exec_path),
        env_path = shell_escape_systemd_path(env_path),
    )
}

fn render_launchd_service(
    label: &str,
    exec_path: &Path,
    env_path: &Path,
    working_dir: &Path,
) -> String {
    let stdout_path = working_dir.join("runtime/launchd-stdout.log");
    let stderr_path = working_dir.join("runtime/launchd-stderr.log");
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"https://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n  <key>Label</key>\n  <string>{label}</string>\n  <key>ProgramArguments</key>\n  <array>\n    <string>{exec_path}</string>\n    <string>--env</string>\n    <string>{env_path}</string>\n    <string>serve</string>\n  </array>\n  <key>RunAtLoad</key>\n  <true/>\n  <key>KeepAlive</key>\n  <true/>\n  <key>ProcessType</key>\n  <string>Background</string>\n  <key>WorkingDirectory</key>\n  <string>{working_dir}</string>\n  <key>StandardOutPath</key>\n  <string>{stdout_path}</string>\n  <key>StandardErrorPath</key>\n  <string>{stderr_path}</string>\n</dict>\n</plist>\n",
        label = xml_escape(label),
        exec_path = xml_escape(&exec_path.display().to_string()),
        env_path = xml_escape(&env_path.display().to_string()),
        working_dir = xml_escape(&working_dir.display().to_string()),
        stdout_path = xml_escape(&stdout_path.display().to_string()),
        stderr_path = xml_escape(&stderr_path.display().to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn systemd_output_includes_exec_and_env() {
        let output = render_service(
            ServiceManager::Systemd,
            "stonr",
            Path::new("/usr/local/bin/stonr"),
            Path::new("/etc/stonr/relay.env"),
            Path::new("/var/lib/stonr"),
        )
        .unwrap();
        assert!(output.contains("[Service]"));
        assert!(output.contains("ExecStart=/usr/local/bin/stonr --env /etc/stonr/relay.env serve"));
        assert!(output.contains("Restart=always"));
    }

    #[test]
    fn launchd_output_escapes_xml() {
        let output = render_service(
            ServiceManager::Launchd,
            "dev.stonr.test",
            Path::new("/Applications/Stonr & Relay.app/Contents/MacOS/stonr"),
            Path::new("/Users/test/Library/Application Support/stonr/relay.env"),
            Path::new("/Users/test/Library/Application Support/stonr"),
        )
        .unwrap();
        assert!(output.contains("<key>ProgramArguments</key>"));
        assert!(output.contains("Stonr &amp; Relay.app"));
        assert!(output.contains("<string>serve</string>"));
    }
}
