use anyhow::{ensure, Context, Result};
use chrono::{Duration, Utc};
use std::io::prelude::*;
use std::net::{TcpStream, UdpSocket};

const TIME_EPOCH: i64 = 2_208_988_800;
const DEFAULT_TIMEOUT: std::time::Duration = std::time::Duration::new(1, 500);

/// Gets the time offset from a RFC868 server over TCP
pub fn get_time_tcp(host: &str, port: u16) -> Result<Duration> {
    let mut socket = TcpStream::connect((host, port))
        .with_context(|| format!("Failed to connect to time server {}.", host))?;
    let mut buf = [0; 4];
    socket.set_read_timeout(Some(DEFAULT_TIMEOUT))?;
    let received = socket
        .read(&mut buf)
        .context("Failed to receive server response")?;
    ensure!(
        received == 4,
        "Server returned a bad response (expected length 4, got {received})"
    );
    let server_time: i64 = u32::from_be_bytes(buf) as i64 - TIME_EPOCH;
    Ok(Duration::milliseconds(
        server_time * 1000 - Utc::now().timestamp_millis(),
    ))
}

/// Gets the time offset from a RFC868 server over UDP
pub fn get_time_udp(host: &str, port: u16) -> Result<Duration> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket
        .connect((host, port))
        .with_context(|| format!("Failed to connect to time server {}.", host))?;
    socket.set_read_timeout(Some(DEFAULT_TIMEOUT))?;
    socket
        .send("".as_bytes())
        .context("Failed to send query to time server")?;
    let mut buf = [0; 4];
    let received = socket
        .recv(&mut buf)
        .context("Failed to receive server response")?;
    ensure!(
        received == 4,
        "Server returned a bad response (expected length 4, got {received})"
    );

    let seconds = u32::from_be_bytes(buf) as i64;
    // some servers return 0 when signaling an error
    ensure!(seconds != 0, "Server returned a bad response");
    let server_time: i64 = seconds - TIME_EPOCH;
    Ok(Duration::milliseconds(
        server_time * 1000 - Utc::now().timestamp_millis(),
    ))
}
