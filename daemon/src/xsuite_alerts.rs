use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, FixedOffset, Utc};
use log::{info, warn};
use rayhunter::analysis::analyzer::EventType;
use serde::Serialize;
use tokio::sync::{RwLock, mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::config::XsuiteAlertsConfig;

const LAN_PROTOCOL: &str = "x-suite-lan-alert";
const LAN_PROTOCOL_VERSION: u8 = 1;
const EVENT_TYPE_CODE_RAYHUNTER: u8 = 200;
const SENSOR_TYPE_SEVERITY: u8 = 200;
const SENSOR_TYPE_HEURISTIC: u8 = 201;
const SENSOR_TYPE_OCCURRENCES: u8 = 202;
const SENSOR_TYPE_TEST: u8 = 203;
const MAX_DATAGRAM_BYTES: usize = 8 * 1024;

static PACKET_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub struct DetectionOccurrence {
    pub severity: EventType,
    pub analyzer_name: String,
    pub analyzer_version: u32,
    pub message: String,
    pub packet_timestamp: Option<DateTime<FixedOffset>>,
    pub recording_id: String,
    pub location: Option<(f64, f64)>,
    pub test: bool,
}

impl DetectionOccurrence {
    pub fn test(location: Option<(f64, f64)>) -> Self {
        Self {
            severity: EventType::High,
            analyzer_name: "Integration test".to_string(),
            analyzer_version: 1,
            message: "This is a test alert. Rayhunter LAN delivery is working.".to_string(),
            packet_timestamp: None,
            recording_id: "TEST".to_string(),
            location,
            test: true,
        }
    }
}

pub enum XsuiteAlertCommand {
    Detection(DetectionOccurrence),
    Test {
        occurrence: DetectionOccurrence,
        response_tx: oneshot::Sender<Result<DeliveryReport, String>>,
    },
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "apidocs", derive(utoipa::ToSchema))]
pub struct DeliveryReport {
    pub correlation_id: String,
    pub packet_count: usize,
    pub broadcast_datagrams: usize,
    pub http_destinations: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
#[cfg_attr(feature = "apidocs", derive(utoipa::ToSchema))]
pub struct XsuiteAlertStatus {
    pub enabled: bool,
    pub last_attempt_at: Option<String>,
    pub last_success_at: Option<String>,
    pub last_error: Option<String>,
    pub last_correlation_id: Option<String>,
    pub sent_alerts: u64,
    pub deduped_alerts: u64,
}

#[derive(Debug, Serialize)]
struct LanClient<'a> {
    id: String,
    app: &'static str,
    role: &'static str,
    label: &'a str,
    unit_id: u16,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AlertData<'a> {
    producer: &'static str,
    correlation_id: &'a str,
    severity: &'static str,
    severity_code: u8,
    heuristic: &'a str,
    heuristic_version: u32,
    heuristic_id: i16,
    message: &'a str,
    recording_id: &'a str,
    occurred_at: i64,
    test: bool,
    location: Option<AlertLocation>,
}

#[derive(Debug, Clone, Copy, Serialize)]
struct AlertLocation {
    lat: f64,
    lon: f64,
}

#[derive(Debug, Serialize)]
struct LanEnvelope<'a> {
    protocol: &'static str,
    version: u8,
    client: LanClient<'a>,
    kind: &'static str,
    text: &'a str,
    data: AlertData<'a>,
}

pub fn run_xsuite_alert_worker(
    task_tracker: &TaskTracker,
    shutdown: CancellationToken,
    config: XsuiteAlertsConfig,
    mut receiver: mpsc::Receiver<XsuiteAlertCommand>,
    status: Arc<RwLock<XsuiteAlertStatus>>,
) {
    task_tracker.spawn(async move {
        status.write().await.enabled = config.enabled;
        let client = crate::http_client::client().ok();
        let mut recent: HashMap<String, (Instant, EventType)> = HashMap::new();

        loop {
            tokio::select! {
                _ = shutdown.cancelled() => return,
                command = receiver.recv() => {
                    let Some(command) = command else { return };
                    let (occurrence, response_tx) = match command {
                        XsuiteAlertCommand::Detection(occurrence) => (occurrence, None),
                        XsuiteAlertCommand::Test { occurrence, response_tx } => (occurrence, Some(response_tx)),
                    };

                    if !config.enabled {
                        if let Some(tx) = response_tx {
                            let _ = tx.send(Err("X Suite LAN alerts are disabled".to_string()));
                        }
                        continue;
                    }
                    if occurrence.severity < config.minimum_severity && !occurrence.test {
                        continue;
                    }

                    let dedupe_key = format!("{}\u{0}{}", occurrence.analyzer_name, occurrence.message);
                    let now = Instant::now();
                    recent.retain(|_, (at, _)| now.duration_since(*at) <= Duration::from_secs(config.dedupe_window_seconds.max(1) * 2));
                    if !occurrence.test
                        && let Some((last_at, last_severity)) = recent.get(&dedupe_key)
                        && now.duration_since(*last_at) < Duration::from_secs(config.dedupe_window_seconds)
                        && occurrence.severity <= *last_severity
                    {
                        status.write().await.deduped_alerts += 1;
                        continue;
                    }
                    recent.insert(dedupe_key, (now, occurrence.severity));

                    let attempted_at = Utc::now().to_rfc3339();
                    status.write().await.last_attempt_at = Some(attempted_at);
                    let result = deliver(&config, client.as_ref(), &occurrence).await;
                    match &result {
                        Ok(report) => {
                            info!("sent X Suite LAN alert {}", report.correlation_id);
                            let mut guard = status.write().await;
                            guard.last_success_at = Some(Utc::now().to_rfc3339());
                            guard.last_error = None;
                            guard.last_correlation_id = Some(report.correlation_id.clone());
                            guard.sent_alerts += 1;
                        }
                        Err(error) => {
                            warn!("failed to send X Suite LAN alert: {error}");
                            status.write().await.last_error = Some(error.clone());
                        }
                    }
                    if let Some(tx) = response_tx {
                        let _ = tx.send(result);
                    }
                }
            }
        }
    });
}

async fn deliver(
    config: &XsuiteAlertsConfig,
    http_client: Option<&reqwest::Client>,
    occurrence: &DetectionOccurrence,
) -> Result<DeliveryReport, String> {
    let correlation_id = generate_correlation_id();
    let occurred_at = occurrence
        .packet_timestamp
        .map(|timestamp| timestamp.timestamp_millis())
        .unwrap_or_else(now_millis);
    let location = occurrence
        .location
        .filter(|(lat, lon)| valid_location(*lat, *lon));
    let event_id = format!("{}09", &correlation_id[..6]);
    let sentinel_id = format!("{}11", &correlation_id[..6]);
    let event_packet = encode_event_packet(config, occurrence, occurred_at, location, &event_id);
    let sentinel_packet = if config.include_sentinel_packet {
        location.map(|coords| {
            encode_sentinel_packet(config, occurrence, occurred_at, coords, &sentinel_id)
        })
    } else {
        None
    };
    let text = match &sentinel_packet {
        Some(packet) => format!("{event_packet}\n{packet}"),
        None => event_packet,
    };
    let heuristic_id = stable_heuristic_id(&occurrence.analyzer_name);
    let message = if config.include_full_message {
        occurrence.message.as_str()
    } else {
        "Detection details hidden by Rayhunter configuration"
    };
    let envelope = LanEnvelope {
        protocol: LAN_PROTOCOL,
        version: LAN_PROTOCOL_VERSION,
        client: LanClient {
            id: format!("rayhunter-{}", normalized_label(&config.device_label)),
            app: "rayhunter",
            role: "client",
            label: &config.device_label,
            unit_id: config.source_unit_id,
        },
        kind: "packet",
        text: &text,
        data: AlertData {
            producer: "rayhunter",
            correlation_id: &correlation_id,
            severity: severity_name(occurrence.severity),
            severity_code: occurrence.severity as u8,
            heuristic: &occurrence.analyzer_name,
            heuristic_version: occurrence.analyzer_version,
            heuristic_id,
            message,
            recording_id: &occurrence.recording_id,
            occurred_at,
            test: occurrence.test,
            location: location.map(|(lat, lon)| AlertLocation { lat, lon }),
        },
    };
    let body = serde_json::to_vec(&envelope).map_err(|error| error.to_string())?;
    if body.len() > MAX_DATAGRAM_BYTES {
        return Err(format!(
            "alert envelope is too large ({} bytes)",
            body.len()
        ));
    }

    let mut broadcast_datagrams = 0usize;
    let mut errors = Vec::new();
    if config.broadcast_enabled {
        match send_broadcast(config, &body).await {
            Ok(sent) => broadcast_datagrams = sent,
            Err(error) => errors.push(error),
        }
    }

    let mut http_destinations = 0usize;
    for destination in &config.destinations {
        let base = destination.trim().trim_end_matches('/');
        if base.is_empty() {
            continue;
        }
        let Some(client) = http_client else {
            errors.push(format!("HTTP client unavailable for {base}"));
            continue;
        };
        let url = format!("{base}/send");
        match client
            .post(&url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.clone())
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => http_destinations += 1,
            Ok(response) => errors.push(format!("{url} returned {}", response.status())),
            Err(error) => errors.push(format!("{url}: {error}")),
        }
    }

    if broadcast_datagrams == 0 && http_destinations == 0 {
        if errors.is_empty() {
            return Err("no X Suite LAN delivery transport is enabled".to_string());
        }
        return Err(errors.join("; "));
    }

    Ok(DeliveryReport {
        correlation_id,
        packet_count: if sentinel_packet.is_some() { 2 } else { 1 },
        broadcast_datagrams,
        http_destinations,
    })
}

async fn send_broadcast(config: &XsuiteAlertsConfig, body: &[u8]) -> Result<usize, String> {
    let configured_target: SocketAddr = format!(
        "{}:{}",
        config.broadcast_address.trim(),
        config.broadcast_port
    )
    .parse()
    .map_err(|error| format!("invalid broadcast address: {error}"))?;
    let socket =
        UdpSocket::bind("0.0.0.0:0").map_err(|error| format!("UDP bind failed: {error}"))?;
    socket
        .set_broadcast(true)
        .map_err(|error| format!("enabling UDP broadcast failed: {error}"))?;
    let mut targets = vec![configured_target];
    let mut target_set = HashSet::from([configured_target]);
    let mut discovery_error = None;
    match if_addrs::get_if_addrs() {
        Ok(interfaces) => {
            for interface in interfaces {
                if interface.is_loopback() || interface.is_link_local() {
                    continue;
                }
                let if_addrs::IfAddr::V4(address) = interface.addr else {
                    continue;
                };
                let Some(broadcast) = address.broadcast else {
                    continue;
                };
                if broadcast.is_unspecified() || broadcast.is_loopback() {
                    continue;
                }
                let target = SocketAddr::new(IpAddr::V4(broadcast), config.broadcast_port);
                if target_set.insert(target) {
                    targets.push(target);
                }
            }
        }
        Err(error) => discovery_error = Some(format!("interface discovery failed: {error}")),
    }

    let repeats = config.broadcast_repeats.clamp(1, 10);
    let mut sent = 0usize;
    let mut errors = Vec::new();
    for index in 0..repeats {
        for target in &targets {
            match socket.send_to(body, target) {
                Ok(_) => sent += 1,
                Err(error) => errors.push(format!("UDP broadcast to {target} failed: {error}")),
            }
        }
        if index + 1 < repeats {
            tokio::time::sleep(Duration::from_millis(
                config.broadcast_repeat_delay_ms.clamp(50, 10_000),
            ))
            .await;
        }
    }
    if sent == 0 {
        if let Some(error) = discovery_error {
            errors.push(error);
        }
        return Err(errors.join("; "));
    }
    if !errors.is_empty() {
        warn!(
            "X Suite LAN alert used a working interface broadcast after another target failed: {}",
            errors.join("; ")
        );
    }
    Ok(sent)
}

fn encode_event_packet(
    config: &XsuiteAlertsConfig,
    occurrence: &DetectionOccurrence,
    occurred_at: i64,
    location: Option<(f64, f64)>,
    packet_id: &str,
) -> String {
    let label = if occurrence.test {
        "RAYHUNTER TEST ALERT"
    } else {
        "POSSIBLE CELL-SITE SIMULATOR"
    };
    let note = if config.include_full_message {
        format!(
            "{}: {} - {}",
            severity_name(occurrence.severity),
            occurrence.analyzer_name,
            occurrence.message
        )
    } else {
        format!(
            "{} detection from Rayhunter",
            severity_name(occurrence.severity)
        )
    };
    let label = truncate_utf8(label, 48);
    let location_label = truncate_utf8(&config.device_label, 48);
    let note = truncate_utf8(&note, 160);

    let mut flags = 2 | 4 | 8;
    if location.is_some() {
        flags |= 1;
    }
    let mut bytes = Vec::with_capacity(13 + label.len() + location_label.len() + note.len() + 11);
    bytes.push(1);
    bytes.extend_from_slice(&config.source_unit_id.to_be_bytes());
    bytes.extend_from_slice(&0u16.to_be_bytes());
    bytes.push(priority_for(occurrence.severity));
    bytes.push(1); // ACTIVE
    bytes.extend_from_slice(&unix_minutes(occurred_at).to_be_bytes());
    bytes.push(EVENT_TYPE_CODE_RAYHUNTER);
    bytes.push(flags);
    if let Some((lat, lon)) = location {
        bytes.extend_from_slice(&coordinate_e5(lat).to_be_bytes());
        bytes.extend_from_slice(&coordinate_e5(lon).to_be_bytes());
    }
    push_short_string(&mut bytes, &label);
    push_short_string(&mut bytes, &location_label);
    push_short_string(&mut bytes, &note);
    format!("X1.9.C.{packet_id}.1/1.{}", base64url(&bytes))
}

fn encode_sentinel_packet(
    config: &XsuiteAlertsConfig,
    occurrence: &DetectionOccurrence,
    occurred_at: i64,
    (lat, lon): (f64, f64),
    packet_id: &str,
) -> String {
    let label = truncate_utf8(&config.device_label, 32);
    let heuristic_id = stable_heuristic_id(&occurrence.analyzer_name);
    let sensors = [
        (SENSOR_TYPE_SEVERITY, occurrence.severity as i16),
        (SENSOR_TYPE_HEURISTIC, heuristic_id),
        (SENSOR_TYPE_OCCURRENCES, 1),
        (SENSOR_TYPE_TEST, i16::from(occurrence.test)),
    ];
    let node_id = if config.node_id == 0 {
        stable_node_id(&config.device_label)
    } else {
        config.node_id
    };
    let mut bytes = Vec::with_capacity(20 + label.len() + sensors.len() * 3);
    bytes.push(1);
    bytes.push(sensors.len() as u8);
    bytes.extend_from_slice(&unix_minutes(occurred_at).to_be_bytes());
    bytes.extend_from_slice(&coordinate_e5(lat).to_be_bytes());
    bytes.extend_from_slice(&coordinate_e5(lon).to_be_bytes());
    bytes.extend_from_slice(&node_id.to_be_bytes());
    bytes.push(1 | 4); // alert + label
    push_short_string(&mut bytes, &label);
    for (sensor_type, value) in sensors {
        bytes.push(sensor_type);
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    format!("X1.11.C.{packet_id}.1/1.{}", base64url(&bytes))
}

fn valid_location(lat: f64, lon: f64) -> bool {
    lat.is_finite()
        && lon.is_finite()
        && (-90.0..=90.0).contains(&lat)
        && (-180.0..=180.0).contains(&lon)
}

fn coordinate_e5(value: f64) -> i32 {
    (value * 100_000.0)
        .round()
        .clamp(i32::MIN as f64, i32::MAX as f64) as i32
}

fn unix_minutes(timestamp_ms: i64) -> u32 {
    (timestamp_ms.max(0) / 60_000).clamp(0, u32::MAX as i64) as u32
}

fn priority_for(severity: EventType) -> u8 {
    match severity {
        EventType::High => 0,
        EventType::Medium => 1,
        EventType::Low => 2,
        EventType::Informational => 3,
    }
}

fn severity_name(severity: EventType) -> &'static str {
    match severity {
        EventType::Informational => "Informational",
        EventType::Low => "Low",
        EventType::Medium => "Medium",
        EventType::High => "High",
    }
}

fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

fn push_short_string(bytes: &mut Vec<u8>, value: &str) {
    bytes.push(value.len() as u8);
    bytes.extend_from_slice(value.as_bytes());
}

fn normalized_label(label: &str) -> String {
    let normalized: String = label
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || *character == '-')
        .take(32)
        .collect();
    if normalized.is_empty() {
        "device".to_string()
    } else {
        normalized
    }
}

fn stable_heuristic_id(value: &str) -> i16 {
    (fnv1a(value.as_bytes()) % 32_767).max(1) as i16
}

fn stable_node_id(value: &str) -> u32 {
    let id = fnv1a(value.as_bytes());
    if id == 0 { 1 } else { id }
}

fn fnv1a(bytes: &[u8]) -> u32 {
    let mut hash = 0x811c9dc5u32;
    for byte in bytes {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x01000193);
    }
    hash
}

fn generate_correlation_id() -> String {
    const ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let counter = PACKET_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut value = (now_millis() as u64).rotate_left(17) ^ counter;
    let mut output = [b'0'; 8];
    for index in (0..output.len()).rev() {
        output[index] = ALPHABET[(value % 36) as usize];
        value /= 36;
    }
    String::from_utf8_lossy(&output).into_owned()
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut output = String::with_capacity((bytes.len() * 4).div_ceil(3));
    let mut index = 0;
    while index + 3 <= bytes.len() {
        let value = (u32::from(bytes[index]) << 16)
            | (u32::from(bytes[index + 1]) << 8)
            | u32::from(bytes[index + 2]);
        output.push(ALPHABET[((value >> 18) & 63) as usize] as char);
        output.push(ALPHABET[((value >> 12) & 63) as usize] as char);
        output.push(ALPHABET[((value >> 6) & 63) as usize] as char);
        output.push(ALPHABET[(value & 63) as usize] as char);
        index += 3;
    }
    match bytes.len() - index {
        1 => {
            let value = u32::from(bytes[index]) << 16;
            output.push(ALPHABET[((value >> 18) & 63) as usize] as char);
            output.push(ALPHABET[((value >> 12) & 63) as usize] as char);
        }
        2 => {
            let value = (u32::from(bytes[index]) << 16) | (u32::from(bytes[index + 1]) << 8);
            output.push(ALPHABET[((value >> 18) & 63) as usize] as char);
            output.push(ALPHABET[((value >> 12) & 63) as usize] as char);
            output.push(ALPHABET[((value >> 6) & 63) as usize] as char);
        }
        _ => {}
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn occurrence() -> DetectionOccurrence {
        DetectionOccurrence {
            severity: EventType::High,
            analyzer_name: "IMSI Requested".to_string(),
            analyzer_version: 2,
            message: "Identity request observed".to_string(),
            packet_timestamp: None,
            recording_id: "123".to_string(),
            location: Some((43.6532, -79.3832)),
            test: false,
        }
    }

    #[test]
    fn base64url_matches_known_vectors() {
        assert_eq!(base64url(b""), "");
        assert_eq!(base64url(b"f"), "Zg");
        assert_eq!(base64url(b"fo"), "Zm8");
        assert_eq!(base64url(b"foo"), "Zm9v");
        assert_eq!(base64url(&[0xfb, 0xff, 0xff]), "-___");
    }

    #[test]
    fn event_and_sentinel_packets_use_native_wrappers() {
        let config = XsuiteAlertsConfig::default();
        let detection = occurrence();
        let event = encode_event_packet(
            &config,
            &detection,
            1_700_000_000_000,
            detection.location,
            "ABCDEF09",
        );
        let sentinel = encode_sentinel_packet(
            &config,
            &detection,
            1_700_000_000_000,
            detection.location.unwrap(),
            "ABCDEF11",
        );
        assert!(event.starts_with("X1.9.C.ABCDEF09.1/1."));
        assert!(sentinel.starts_with("X1.11.C.ABCDEF11.1/1."));
        assert_eq!(
            event,
            "X1.9.C.ABCDEF09.1/1.Af3oAAAAAQGwVRXIDwBCnAj_ht7wHFBPU1NJQkxFIENFTEwtU0lURSBTSU1VTEFUT1IJUkFZSFVOVEVSMEhpZ2g6IElNU0kgUmVxdWVzdGVkIC0gSWRlbnRpdHkgcmVxdWVzdCBvYnNlcnZlZA"
        );
        assert_eq!(
            sentinel,
            "X1.11.C.ABCDEF11.1/1.AQQBsFUVAEKcCP-G3vAAGOuNBQlSQVlIVU5URVLIAAPJJhXKAAHLAAA"
        );
        assert!(event.len() < 512);
        assert!(sentinel.len() < 256);
    }

    #[test]
    fn utf8_truncation_never_splits_a_character() {
        assert_eq!(truncate_utf8("abc😀def", 6), "abc");
        assert_eq!(truncate_utf8("abc😀def", 7), "abc😀");
    }

    #[test]
    fn locations_are_validated() {
        assert!(valid_location(0.0, 0.0));
        assert!(valid_location(90.0, -180.0));
        assert!(!valid_location(91.0, 0.0));
        assert!(!valid_location(f64::NAN, 0.0));
    }
}
