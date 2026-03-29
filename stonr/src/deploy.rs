use std::path::Path;

use anyhow::Result;
use clap::ValueEnum;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ServiceManager {
    Systemd,
    Launchd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ProxyManager {
    Caddy,
    Nginx,
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

pub fn render_proxy(
    manager: ProxyManager,
    domain: &str,
    http_upstream: &str,
    ws_upstream: &str,
    tls_cert: Option<&str>,
    tls_key: Option<&str>,
) -> Result<String> {
    Ok(match manager {
        ProxyManager::Caddy => render_caddy_proxy(domain, http_upstream, ws_upstream),
        ProxyManager::Nginx => {
            render_nginx_proxy(domain, http_upstream, ws_upstream, tls_cert, tls_key)?
        }
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

fn render_caddy_proxy(domain: &str, http_upstream: &str, ws_upstream: &str) -> String {
    format!(
        "{domain} {{\n  encode zstd gzip\n\n  @websocket {{\n    header Connection *Upgrade*\n    header Upgrade websocket\n  }}\n\n  reverse_proxy @websocket {ws_upstream}\n  reverse_proxy {http_upstream}\n\n  header {{\n    Strict-Transport-Security \"max-age=31536000; includeSubDomains; preload\"\n  }}\n}}\n",
        domain = domain,
        http_upstream = http_upstream,
        ws_upstream = ws_upstream,
    )
}

fn render_nginx_proxy(
    domain: &str,
    http_upstream: &str,
    ws_upstream: &str,
    tls_cert: Option<&str>,
    tls_key: Option<&str>,
) -> Result<String> {
    let tls_cert = tls_cert.ok_or_else(|| anyhow::anyhow!("--tls-cert is required for nginx"))?;
    let tls_key = tls_key.ok_or_else(|| anyhow::anyhow!("--tls-key is required for nginx"))?;
    Ok(format!(
        "map $http_upgrade $stonr_connection_upgrade {{\n    default upgrade;\n    '' close;\n}}\n\nmap $http_upgrade $stonr_upstream {{\n    default {ws_upstream};\n    '' {http_upstream};\n}}\n\nserver {{\n    listen 443 ssl http2;\n    server_name {domain};\n\n    ssl_certificate {tls_cert};\n    ssl_certificate_key {tls_key};\n\n    location / {{\n        proxy_http_version 1.1;\n        proxy_set_header Host $host;\n        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;\n        proxy_set_header X-Forwarded-Proto https;\n        proxy_set_header Upgrade $http_upgrade;\n        proxy_set_header Connection $stonr_connection_upgrade;\n        proxy_pass http://$stonr_upstream;\n    }}\n}}\n",
        domain = domain,
        http_upstream = http_upstream,
        ws_upstream = ws_upstream,
        tls_cert = tls_cert,
        tls_key = tls_key,
    ))
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

    #[test]
    fn caddy_proxy_routes_websocket_and_http() {
        let output = render_proxy(
            ProxyManager::Caddy,
            "relay.example.com",
            "127.0.0.1:7777",
            "127.0.0.1:7778",
            None,
            None,
        )
        .unwrap();
        assert!(output.contains("@websocket"));
        assert!(output.contains("reverse_proxy @websocket 127.0.0.1:7778"));
        assert!(output.contains("reverse_proxy 127.0.0.1:7777"));
    }

    #[test]
    fn nginx_proxy_requires_tls_paths() {
        let error = render_proxy(
            ProxyManager::Nginx,
            "relay.example.com",
            "127.0.0.1:7777",
            "127.0.0.1:7778",
            None,
            None,
        )
        .unwrap_err();
        assert!(error.to_string().contains("--tls-cert"));
    }

    #[test]
    fn nginx_proxy_includes_tls_and_upgrade_routing() {
        let output = render_proxy(
            ProxyManager::Nginx,
            "relay.example.com",
            "127.0.0.1:7777",
            "127.0.0.1:7778",
            Some("/etc/letsencrypt/live/relay.example.com/fullchain.pem"),
            Some("/etc/letsencrypt/live/relay.example.com/privkey.pem"),
        )
        .unwrap();
        assert!(output.contains("map $http_upgrade $stonr_upstream"));
        assert!(output.contains("default 127.0.0.1:7778;"));
        assert!(output.contains("'' 127.0.0.1:7777;"));
        assert!(output
            .contains("ssl_certificate /etc/letsencrypt/live/relay.example.com/fullchain.pem;"));
    }
}
