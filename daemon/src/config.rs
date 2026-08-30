use log::warn;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

use rayhunter::Device;
use rayhunter::analysis::analyzer::{AnalyzerConfig, EventType};

use crate::error::RayhunterError;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Serialize_repr, Deserialize_repr)]
#[cfg_attr(feature = "apidocs", derive(utoipa::ToSchema))]
pub enum GpsMode {
    Disabled = 0,
    Fixed = 1,
    Api = 2,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Serialize_repr, Deserialize_repr)]
#[cfg_attr(feature = "apidocs", derive(utoipa::ToSchema))]
pub enum UiLevel {
    Invisible = 0,
    Subtle = 1,
    Demo = 2,
    EffLogo = 3,
    HighVisibility = 4,
    TransFlag = 128,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Serialize_repr, Deserialize_repr)]
#[cfg_attr(feature = "apidocs", derive(utoipa::ToSchema))]
pub enum KeyInputMode {
    Disabled = 0,
    DoubleTapPower = 1,
}
use crate::notifications::NotificationType;

pub const DEFAULT_SCREEN_ALERT_MESSAGE: &str =
    "POSSIBLE CELL-SITE SIMULATOR DETECTED - PRESS KEY TO CLEAR";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
#[cfg_attr(feature = "apidocs", derive(utoipa::ToSchema))]
pub struct ScreenAlertConfig {
    /// Wake and flash the device screen when a warning is detected.
    pub enabled: bool,
    /// Message shown while the warning is flashing.
    pub message: String,
}

impl Default for ScreenAlertConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            message: DEFAULT_SCREEN_ALERT_MESSAGE.to_string(),
        }
    }
}

/// Zero-configuration XTOC/XCOM alert delivery over the local LAN.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
#[cfg_attr(feature = "apidocs", derive(utoipa::ToSchema))]
pub struct XsuiteAlertsConfig {
    /// Master switch. Enabled by default so a newly flashed device advertises detections.
    pub enabled: bool,
    /// UDP limited-broadcast delivery. Packaged XTOC/XCOM receivers listen automatically.
    pub broadcast_enabled: bool,
    pub broadcast_address: String,
    pub broadcast_port: u16,
    /// Optional bridge base URLs for routed/VLAN networks.
    pub destinations: Vec<String>,
    pub device_label: String,
    pub source_unit_id: u16,
    /// Zero derives a stable node id from device_label.
    pub node_id: u32,
    pub minimum_severity: EventType,
    pub include_sentinel_packet: bool,
    pub include_full_message: bool,
    pub dedupe_window_seconds: u64,
    pub broadcast_repeats: u8,
    pub broadcast_repeat_delay_ms: u64,
}

impl Default for XsuiteAlertsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            broadcast_enabled: true,
            broadcast_address: "255.255.255.255".to_string(),
            broadcast_port: 8096,
            destinations: Vec::new(),
            device_label: "RAYHUNTER".to_string(),
            source_unit_id: 65_000,
            node_id: 0,
            minimum_severity: EventType::Low,
            include_sentinel_packet: true,
            include_full_message: true,
            dedupe_window_seconds: 300,
            broadcast_repeats: 3,
            broadcast_repeat_delay_ms: 750,
        }
    }
}

/// The structure of a valid rayhunter configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
#[cfg_attr(feature = "apidocs", derive(utoipa::ToSchema))]
pub struct Config {
    /// Path to store QMDL files
    pub qmdl_store_path: String,
    /// Listening port
    pub port: u16,
    /// Debug mode
    pub debug_mode: bool,
    /// Internal device name
    pub device: Device,
    /// UI level
    pub ui_level: UiLevel,
    /// Colorblind mode
    pub colorblind_mode: bool,
    /// Key input mode
    pub key_input_mode: KeyInputMode,
    /// Latched on-device warning alert configuration
    pub screen_alert: ScreenAlertConfig,
    /// Native X Suite LAN alerts for XTOC and XCOM.
    pub xsuite_alerts: XsuiteAlertsConfig,
    /// ntfy.sh URL
    pub ntfy_url: Option<String>,
    /// Vector containing the types of enabled notifications
    pub enabled_notifications: Vec<NotificationType>,
    /// Whether Rayhunter should periodically check GitHub for new releases
    pub auto_check_updates: bool,
    /// Vector containing the list of enabled analyzers
    pub analyzers: AnalyzerConfig,
    /// Minimum disk space required to start a recording
    pub min_space_to_start_recording_mb: u64,
    /// Minimum disk space required to continue a recording
    pub min_space_to_continue_recording_mb: u64,
    /// GPS mode
    pub gps_mode: GpsMode,
    /// Fixed latitude used when gps_mode=1
    pub gps_fixed_latitude: Option<f64>,
    /// Fixed longitude used when gps_mode=1
    pub gps_fixed_longitude: Option<f64>,
    /// Wifi client SSID
    pub wifi_ssid: Option<String>,
    /// Wifi client password
    pub wifi_password: Option<String>,
    /// Wifi security type (wpa_psk or sae)
    pub wifi_security: Option<wifi_station::SecurityType>,
    /// Wifi client mode
    pub wifi_enabled: bool,
    /// Vector containing wifi client DNS servers
    pub dns_servers: Option<Vec<String>>,
    /// WebDAV upload configuration. The upload worker runs whenever `webdav.url` is non-empty.
    pub webdav: WebdavConfig,
}

/// Configuration for uploading finished QMDL recordings to a WebDAV server.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
#[cfg_attr(feature = "apidocs", derive(utoipa::ToSchema))]
pub struct WebdavConfig {
    /// WebDAV server base URL, e.g. "https://example.com/remote.php/files/untitaker/my-subfolder/"
    pub url: String,
    /// Optional username for HTTP Basic auth
    pub username: Option<String>,
    /// Optional password for HTTP Basic auth
    pub password: Option<String>,
    /// Timeout (in seconds) for each upload request
    pub upload_timeout_secs: u64,
    /// How often (in seconds) the worker scans for entries to upload
    pub poll_interval_secs: u64,
    /// Minimum age (in seconds) an entry must have before it becomes eligible for upload
    pub min_age_secs: i64,
    /// Delete the file locally after a successful upload
    pub delete_on_upload: bool,
}

impl Default for WebdavConfig {
    fn default() -> Self {
        WebdavConfig {
            url: String::new(),
            username: None,
            password: None,
            upload_timeout_secs: 300,
            poll_interval_secs: 3600,
            min_age_secs: 86400,
            delete_on_upload: false,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            qmdl_store_path: "/data/rayhunter/qmdl".to_string(),
            port: 8080,
            debug_mode: false,
            device: Device::Orbic,
            ui_level: UiLevel::Subtle,
            colorblind_mode: false,
            key_input_mode: KeyInputMode::Disabled,
            screen_alert: ScreenAlertConfig::default(),
            xsuite_alerts: XsuiteAlertsConfig::default(),
            analyzers: AnalyzerConfig::default(),
            ntfy_url: None,
            enabled_notifications: vec![NotificationType::Warning, NotificationType::LowBattery],
            auto_check_updates: true,
            min_space_to_start_recording_mb: 1,
            min_space_to_continue_recording_mb: 1,
            gps_mode: GpsMode::Disabled,
            gps_fixed_latitude: None,
            gps_fixed_longitude: None,
            wifi_ssid: None,
            wifi_password: None,
            wifi_security: None,
            wifi_enabled: false,
            dns_servers: None,
            webdav: WebdavConfig::default(),
        }
    }
}

impl Config {
    pub fn wifi_config(&self) -> wifi_station::WifiConfig {
        let (wpa_bin, hostapd_conf, ctrl_interface) = match self.device {
            Device::Tmobile | Device::Wingtech => (
                Some("/usr/sbin/wpa_supplicant".into()),
                Some("/data/configs/hostapd.conf".into()),
                None,
            ),
            Device::Uz801 => (
                Some("/system/bin/wpa_supplicant".into()),
                Some("/data/misc/wifi/hostapd.conf".into()),
                Some("/data/misc/wifi/sockets".into()),
            ),
            _ => (None, None, None),
        };
        wifi_station::WifiConfig {
            wifi_enabled: self.wifi_enabled,
            dns_servers: self.dns_servers.clone(),
            wifi_ssid: self.wifi_ssid.clone(),
            wifi_password: self.wifi_password.clone(),
            security_type: self.wifi_security,
            wpa_supplicant_bin: wpa_bin.or_else(|| resolve_bin("wpa_supplicant")),
            hostapd_conf,
            ctrl_interface,
            udhcpc_hook_path: Some("/data/rayhunter/udhcpc-hook.sh".into()),
            dhcp_lease_path: Some("/data/rayhunter/dhcp_lease".into()),
            wpa_conf_path: Some("/data/rayhunter/wpa_sta.conf".into()),
            iw_bin: resolve_bin("iw"),
            udhcpc_bin: resolve_bin("udhcpc"),
            crash_log_dir: Some("/data/rayhunter/crash-logs".into()),
            wakelock_name: Some("rayhunter".into()),
        }
    }
}

fn resolve_bin(name: &str) -> Option<String> {
    let local = format!("/data/rayhunter/bin/{name}");
    if std::path::Path::new(&local).exists() {
        return Some(local);
    }
    None
}

pub async fn parse_config<P>(path: P) -> Result<Config, RayhunterError>
where
    P: AsRef<std::path::Path>,
{
    let mut config = if let Ok(config_file) = tokio::fs::read_to_string(&path).await {
        toml::from_str(&config_file).map_err(RayhunterError::ConfigFileParsingError)?
    } else {
        warn!("unable to read config file, using default config");
        Config::default()
    };

    if let Some((ssid, security)) =
        wifi_station::read_network_from_wpa_conf("/data/rayhunter/wpa_sta.conf")
    {
        config.wifi_ssid = Some(ssid);
        config.wifi_security = Some(security);
    } else {
        config.wifi_ssid = None;
        config.wifi_security = None;
    }
    config.wifi_password = None;

    Ok(config)
}

pub struct Args {
    pub config_path: String,
}

pub fn parse_args() -> Args {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        println!("Usage: {} /path/to/config/file", args[0]);
        std::process::exit(1);
    }
    Args {
        config_path: args[1].clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distribution_template_keeps_root_settings_at_the_root() {
        let config: Config = toml::from_str(include_str!("../../dist/config.toml.in"))
            .expect("distribution config template should parse");

        assert!(config.screen_alert.enabled);
        assert_eq!(
            config.screen_alert.message,
            DEFAULT_SCREEN_ALERT_MESSAGE.to_string()
        );
        assert!(!config.auto_check_updates);
        assert_eq!(config.min_space_to_start_recording_mb, 1);
        assert_eq!(config.min_space_to_continue_recording_mb, 1);
        assert!(!config.wifi_enabled);
        assert_eq!(
            config.enabled_notifications,
            vec![NotificationType::Warning, NotificationType::LowBattery]
        );
    }
}
