//! Tests for transport configuration and factory

use crate::transport::*;
use std::path::PathBuf;

#[test]
fn test_transport_type_serialization() {
    // Test serialization/deserialization of transport types
    let ssh_type = TransportType::Ssh;
    let local_type = TransportType::Local;
    let tcp_type = TransportType::Tcp;
    let wsl_type = TransportType::Wsl;

    // Test string conversion
    assert_eq!(ssh_type.to_string(), "ssh");
    assert_eq!(local_type.to_string(), "local");
    assert_eq!(tcp_type.to_string(), "tcp");
    assert_eq!(wsl_type.to_string(), "wsl");

    // Test parsing
    assert_eq!("ssh".parse::<TransportType>().unwrap(), TransportType::Ssh);
    assert_eq!(
        "local".parse::<TransportType>().unwrap(),
        TransportType::Local
    );
    assert_eq!("tcp".parse::<TransportType>().unwrap(), TransportType::Tcp);
    assert_eq!("wsl".parse::<TransportType>().unwrap(), TransportType::Wsl);
}

#[test]
fn test_transport_config_defaults() {
    let config = TransportConfig::default();
    assert_eq!(config.transport_type, TransportType::Local);
    assert!(config.local.is_some());
    assert!(config.ssh.is_none());
    assert!(config.tcp.is_none());
    assert!(config.wsl.is_none());
}

#[test]
fn test_ssh_transport_config() {
    let ssh_config = SshConfig {
        host: "example.com".to_string(),
        port: 22,
        username: "user".to_string(),
        password: Some("pass".to_string()),
        key_path: None,
        auto_upload_binary: false,
        timeout: 30,
        keepalive: 60,
    };

    assert_eq!(ssh_config.host, "example.com");
    assert_eq!(ssh_config.port, 22);
    assert_eq!(ssh_config.username, "user");
    assert_eq!(ssh_config.password, Some("pass".to_string()));
}

#[test]
fn test_local_transport_config() {
    let local_config = LocalConfig {
        binary_path: PathBuf::from("/path/to/yuha-remote"),
        args: vec!["--stdio".to_string(), "--debug".to_string()],
        working_dir: Some(PathBuf::from("/tmp")),
    };

    assert_eq!(
        local_config.binary_path,
        PathBuf::from("/path/to/yuha-remote")
    );
    assert_eq!(local_config.args, vec!["--stdio", "--debug"]);
    assert_eq!(local_config.working_dir, Some(PathBuf::from("/tmp")));
}

#[test]
fn test_tcp_transport_config() {
    let tcp_config = TcpConfig {
        host: "localhost".to_string(),
        port: 9999,
        timeout: 30,
        tls: None,
    };

    assert_eq!(tcp_config.host, "localhost");
    assert_eq!(tcp_config.port, 9999);
    assert_eq!(tcp_config.timeout, 30);
    assert!(tcp_config.tls.is_none());
}

#[test]
fn test_wsl_transport_config() {
    let wsl_config = WslConfig {
        distribution: Some("Ubuntu".to_string()),
        user: Some("user".to_string()),
        binary_path: Some(PathBuf::from("yuha-remote")),
        working_dir: Some(PathBuf::from("/home/user")),
    };

    assert_eq!(wsl_config.distribution, Some("Ubuntu".to_string()));
    assert_eq!(wsl_config.user, Some("user".to_string()));
    assert_eq!(wsl_config.binary_path, Some(PathBuf::from("yuha-remote")));
    assert_eq!(wsl_config.working_dir, Some(PathBuf::from("/home/user")));
}

#[test]
fn test_transport_builder() {
    // Test SSH transport builder
    let ssh_config = TransportBuilder::ssh()
        .host("example.com")
        .port(22)
        .username("user")
        .password("pass")
        .build()
        .unwrap();

    assert_eq!(ssh_config.transport_type, TransportType::Ssh);
    assert!(ssh_config.ssh.is_some());
    assert_eq!(ssh_config.ssh.as_ref().unwrap().host, "example.com");

    // Test local transport builder
    let local_config = TransportBuilder::local()
        .binary_path(PathBuf::from("yuha-remote"))
        .args(vec!["--stdio".to_string()])
        .build()
        .unwrap();

    assert_eq!(local_config.transport_type, TransportType::Local);
    assert!(local_config.local.is_some());

    // Test TCP transport builder
    let tcp_config = TransportBuilder::tcp()
        .host("localhost")
        .port(9999)
        .build()
        .unwrap();

    assert_eq!(tcp_config.transport_type, TransportType::Tcp);
    assert!(tcp_config.tcp.is_some());
}

#[test]
fn test_transport_config_validation() {
    // Test valid SSH config
    let mut ssh_config = TransportConfig {
        transport_type: TransportType::Ssh,
        ssh: Some(SshConfig {
            host: "example.com".to_string(),
            port: 22,
            username: "user".to_string(),
            password: Some("pass".to_string()),
            key_path: None,
            auto_upload_binary: false,
            timeout: 30,
            keepalive: 60,
        }),
        local: None,
        tcp: None,
        wsl: None,
        general: GeneralConfig::default(),
    };
    assert!(ssh_config.validate().is_ok());

    // Test invalid SSH config (empty host)
    ssh_config.ssh.as_mut().unwrap().host = String::new();
    assert!(ssh_config.validate().is_err());

    // Test valid TCP config
    let mut tcp_config = TransportConfig {
        transport_type: TransportType::Tcp,
        ssh: None,
        local: None,
        tcp: Some(TcpConfig {
            host: "localhost".to_string(),
            port: 9999,
            timeout: 30,
            tls: None,
        }),
        wsl: None,
        general: GeneralConfig::default(),
    };
    assert!(tcp_config.validate().is_ok());

    // Test invalid TCP config (port 0)
    tcp_config.tcp.as_mut().unwrap().port = 0;
    assert!(tcp_config.validate().is_err());
}

#[test]
fn test_config_serialization() {
    let config = TransportConfig {
        transport_type: TransportType::Ssh,
        ssh: Some(SshConfig {
            host: "example.com".to_string(),
            port: 22,
            username: "user".to_string(),
            password: None,
            key_path: Some(PathBuf::from("/path/to/key")),
            auto_upload_binary: true,
            timeout: 30,
            keepalive: 60,
        }),
        local: None,
        tcp: None,
        wsl: None,
        general: GeneralConfig::default(),
    };

    // Test TOML serialization
    let toml_str = toml::to_string(&config).expect("Failed to serialize to TOML");
    let deserialized: TransportConfig =
        toml::from_str(&toml_str).expect("Failed to deserialize from TOML");

    assert_eq!(config.transport_type, deserialized.transport_type);
    assert_eq!(
        config.ssh.as_ref().unwrap().host,
        deserialized.ssh.as_ref().unwrap().host
    );
}
