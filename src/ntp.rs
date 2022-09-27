use anyhow::{bail, Context, Result};
use byteorder::{BigEndian, ReadBytesExt};
use chrono::{DateTime, Duration, TimeZone, Utc};
use lazy_static::lazy_static;
use rand::random;
use std::net::UdpSocket;

pub const NTP_PORT: u16 = 123;

#[derive(Debug)]
struct NTPTimestamps {
    t1: DateTime<Utc>, // Origin
    t2: DateTime<Utc>, // Receive
    t3: DateTime<Utc>, // Transmit
    t4: DateTime<Utc>, // Destination
}

impl NTPTimestamps {
    /// Calculate delta
    pub fn delay(&self) -> Duration {
        let mut delta = (self.t4 - self.t1) - (self.t2 - self.t3);
        // if delta is smaller than rho (clock precision), we should set delta=rho

        delta = delta.max(chrono::Duration::nanoseconds((*RHO * 1e9) as i64));
        assert!(delta > chrono::Duration::zero());
        delta
    }

    /// Calculate theta
    pub fn offset(&self) -> Duration {
        ((self.t2 - self.t1) + (self.t3 - self.t4)) / 2
    }
}

#[derive(Default, Debug, Copy, Clone)]
struct NTPPacket {
    leap: u8,
    version: u8,
    mode: u8,
    stratum: u8,
    poll: u8,
    precision: u8,
    root_delay: u32,
    root_dispersion: u32,
    reference_id: u32,
    reference_timestamp: NTPTimestamp,
    origin_timestamp: NTPTimestamp,
    receive_timestamp: NTPTimestamp,
    transmit_timestamp: NTPTimestamp,
}

#[allow(dead_code)]
enum Mode {
    Reserved,
    SymmetricActive,
    SymmetricPassive,
    Client,
    Server,
    Broadcast,
    NTPControlMessage,
    ReservedPrivate,
}

impl NTPPacket {
    const MESSAGE_LENGTH: usize = 48;

    fn parse(mut input: &[u8]) -> Result<NTPPacket, std::io::Error> {
        let mut packet = NTPPacket::default();

        let li_vn_mode = input.read_u8()?;
        packet.leap = (li_vn_mode >> 6) & 0b11;
        packet.version = (li_vn_mode >> 3) & 0b111;
        packet.mode = (li_vn_mode) & 0b111;
        packet.stratum = input.read_u8()?;
        packet.poll = input.read_u8()?;
        packet.precision = input.read_u8()?;
        packet.root_delay = input.read_u32::<BigEndian>()?;
        packet.root_dispersion = input.read_u32::<BigEndian>()?;
        packet.reference_id = input.read_u32::<BigEndian>()?;

        for t in [
            &mut packet.reference_timestamp,
            &mut packet.origin_timestamp,
            &mut packet.receive_timestamp,
            &mut packet.transmit_timestamp,
        ] {
            (*t).seconds = input.read_u32::<BigEndian>()?;
            (*t).fraction = input.read_u32::<BigEndian>()?;
        }
        Ok(packet)
    }

    fn client() -> Self {
        NTPPacket {
            version: 4,
            mode: Mode::Client as u8,
            ..Default::default()
        }
    }

    fn build(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(Self::MESSAGE_LENGTH);
        let li_vn_mode: u8 = self.leap << 6 | self.version << 3 | self.mode;
        data.push(li_vn_mode);
        data.push(self.stratum);
        data.push(self.poll);
        data.push(self.precision);
        data.extend_from_slice(&self.root_delay.to_be_bytes());
        data.extend_from_slice(&self.root_dispersion.to_be_bytes());
        data.extend_from_slice(&self.reference_id.to_be_bytes());

        for t in [
            self.reference_timestamp,
            self.origin_timestamp,
            self.receive_timestamp,
            self.transmit_timestamp,
        ] {
            data.extend_from_slice(&t.seconds.to_be_bytes());
            data.extend_from_slice(&t.fraction.to_be_bytes());
        }

        debug_assert_eq!(data.len(), Self::MESSAGE_LENGTH);
        data
    }
}

fn ntp_roundtrip(host: &str, port: u16) -> Result<NTPTimestamps> {
    let timeout = std::time::Duration::new(1, 0);

    let mut response = [0_u8; NTPPacket::MESSAGE_LENGTH];

    let udp = UdpSocket::bind("0.0.0.0:0")?;
    udp.set_read_timeout(Some(timeout))?;

    udp.connect((host, port))
        .with_context(|| format!("Failed to connect to time server {}.", host))?;
    let mut client = NTPPacket::client();

    // We set a random transmit timestamp and compare it later with the response
    // to detect bogus packets.
    client.transmit_timestamp.seconds = random();
    client.transmit_timestamp.fraction = random();
    let message = client.build();

    // NTP critical section
    let t1 = Utc::now();
    let send_result = udp.send(&message);
    let recv_result = udp.recv_from(&mut response);
    let t4 = Utc::now();
    // end NTP critical section

    // We handle problems outside the critical section
    send_result.context("Failed to send NTP request server")?;
    recv_result.context("Failed to receive NTP response")?;

    let ntp_response = NTPPacket::parse(&response).context("Bad NTP Response")?;

    if ntp_response.mode != Mode::Server as u8 {
        bail!(
            "Bad NTP response (unexpected mode value {})",
            ntp_response.mode
        );
    }
    if ntp_response.stratum == 0 {
        bail!("Bad NTP response (stratum is zero)");
    }
    if ntp_response.transmit_timestamp.seconds == 0 || ntp_response.transmit_timestamp.fraction == 0
    {
        bail!("Bad NTP response (transmit timestamp is zero)");
    }
    if ntp_response.origin_timestamp != client.transmit_timestamp {
        bail!("Bad NTP response (response's origin_timestamp does not equal request's transmit_timestamp)");
    }

    let t2: DateTime<Utc> = ntp_response.receive_timestamp.into();
    let t3: DateTime<Utc> = ntp_response.transmit_timestamp.into();

    Ok(NTPTimestamps { t1, t2, t3, t4 })
}

#[derive(Eq, PartialEq, Default, Debug, Copy, Clone)]
struct NTPTimestamp {
    seconds: u32,
    fraction: u32,
}

const NTP_EPOCH: i64 = 2_208_988_800;

impl From<NTPTimestamp> for DateTime<Utc> {
    fn from(ntp: NTPTimestamp) -> Self {
        let secs = ntp.seconds as i64 - NTP_EPOCH;
        let fraction_to_nano = 1e9 * 2_f64.powi(-32);
        let nanos = ntp.fraction as f64 * fraction_to_nano;
        Utc.timestamp(secs, nanos.round() as u32)
    }
}

pub struct NTPResults {
    results: Vec<(Duration, Duration)>, // (offset, delay)
}

impl NTPResults {
    /// Return the jitter (psi) of the results in nanoseconds
    pub fn jitter(&self) -> f64 {
        let min_offset_by_delay = match self.results.iter().min_by_key(|k| k.1) {
            Some(min) => min.0,
            None => Duration::seconds(0), // This will only happen when self.results is empty, and in this case the following iteration will be trivial anyway
        };
        let psi = self
            .results
            .iter()
            .map(|&x| {
                ((x.0 - min_offset_by_delay)
                    .num_nanoseconds()
                    .expect("This should never overflow") as f64)
                    .powi(2)
            })
            .sum::<f64>()
            .sqrt()
            / 1e9 // return results as seconds
            * (1.0 / (self.results.len() as f64 - 1.0));
        psi
    }

    pub fn min_offset(&self) -> Duration {
        match self.results.iter().min_by_key(|k| k.1) {
            Some(min) => min.0,
            None => Duration::seconds(0), // This will only happen when self.results is empty
        }
    }

    pub fn min_delay(&self) -> Duration {
        match self.results.iter().min_by_key(|k| k.1) {
            Some(min) => min.1,
            None => Duration::seconds(0), // This will only happen when self.results is empty
        }
    }
}
/// Performs an SNTP (RFC 5905) query.
pub fn ntp_query(host: &str, port: u16) -> Result<NTPResults> {
    const NUM_TIMINGS: usize = 8;
    let mut results = NTPResults {
        results: Vec::with_capacity(8),
    };
    for i in 1..25 {
        let ntp_result = match ntp_roundtrip(host, port) {
            Ok(result) => result,
            Err(err) => {
                eprintln!("NTP query failed (attempt {}): {}", i, err);
                continue;
            }
        };
        results
            .results
            .push((ntp_result.offset(), ntp_result.delay()));

        if results.results.len() >= NUM_TIMINGS {
            break;
        }
    }
    match results.results.len() < NUM_TIMINGS {
        true => bail!(
            "Couldn't gather enough successful timings, (gathered {}) ",
            results.results.len()
        ),
        false => Ok(results),
    }
}

/// Returns the precision of the system clock.
///
/// This is system rho from the NTP RFC.
fn clock_precision() -> f64 {
    let mut min_precision: f64 = 1e2;
    for _ in 1..8 {
        let t1 = Utc::now();
        let t2 = Utc::now();
        let precision = (t2 - t1).num_nanoseconds().expect("This can't fail") as f64 / 1e9;
        min_precision = min_precision.min(precision);
    }
    min_precision
}

lazy_static! {
/// The system's clock precision
pub static ref RHO: f64 = clock_precision();
}
