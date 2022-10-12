use anyhow::{bail, Result};
use chrono::Duration;
use clap::{ArgAction, Parser};

mod ntp;
mod rfc868;

/// A simple SNTP (RFC 5905) and RFC 868 client written in Rust.
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Time server (e.g. time.nist.gov)
    host: String,

    /// Verbose output
    #[arg(short, long, action = ArgAction::Count)]
    verbose: u8,

    /// Just print, don't set
    #[arg(short, long, conflicts_with = "silent")]
    print: bool,

    /// Just set, don't print
    #[arg(short, long, conflicts_with = "print")]
    silent: bool,

    /// Use UDP instead of TCP as transport for RFC 868. SNTP will always use UDP protocol.
    #[arg(short)]
    udp: bool,

    /// Use port 'port' instead of port 37 (RFC 868) or 123 (SNTP, RFC 5905).
    #[arg(short = 'o', long)]
    port: Option<u16>,

    /// Use the adjtime(2) call to gradually skew the local time to the remote time rather than
    /// just hopping.
    #[arg(short)]
    adjtime: bool,

    /// Use RFC 868 time protocol instead of SNTP (RFC 5905).
    #[arg(long)]
    rfc868: bool,
}

#[test]
fn verify_app() {
    use clap::CommandFactory;
    Cli::command().debug_assert();
}

trait TimeVal {
    fn timeval(&self) -> libc::timeval;
}

impl TimeVal for Duration {
    /// Convert from chrono::Duration to libc::timeval
    fn timeval(&self) -> libc::timeval {
        let d = self;
        let mut seconds = d.num_seconds();
        let residue = *d - Duration::seconds(seconds); // -1 sec < residue < 1 sec
        let mut micros = residue.num_microseconds().expect("Can't fail");
        if micros < 0 {
            seconds -= 1;
            micros += 1_000_000;
        }

        libc::timeval {
            tv_sec: seconds,
            tv_usec: micros,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_timeval() {
        let tv = Duration::milliseconds(1100).timeval();
        assert_eq!(tv.tv_sec, 1);
        assert_eq!(tv.tv_usec, 100_000);
    }
    #[test]
    fn test_timeval_negative() {
        let tv = Duration::milliseconds(-900).timeval();
        assert_eq!(tv.tv_sec, -1);
        assert_eq!(tv.tv_usec, 100_000);
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    const TIME_PORT: u16 = 37;

    if cli.verbose > 0 {
        let precision = *ntp::RHO;
        println!(
            "Precision: {}μs ({})",
            (precision * 1e3).ceil(),
            precision.log2().ceil()
        );
    }

    let delta = match cli.rfc868 {
        true => {
            let port = cli.port.unwrap_or(TIME_PORT);
            match cli.udp {
                true => rfc868::get_time_udp(&cli.host, port),
                false => rfc868::get_time_tcp(&cli.host, port),
            }?
        }
        false => {
            let port = cli.port.unwrap_or(ntp::NTP_PORT);
            let results = ntp::ntp_query(&cli.host, port)?;
            if cli.verbose > 0 {
                println!("Jitter: {:.1}μs", results.jitter() * 1e6);
                println!("Delay: {}ms", results.min_delay().num_milliseconds());
            }
            results.min_offset()
        }
    };

    let float_delta = delta.num_microseconds().expect("Unreasonable time delta") as f64 / 1e6;
    if !cli.silent {
        println!("{}", (chrono::offset::Local::now() + delta).format("%c"));
        if cli.verbose > 0 {
            println!(
                "adjust local clock by {:.6} seconds ({})",
                float_delta,
                if cli.adjtime {
                    "adjtime"
                } else {
                    "instant change"
                }
            );
        }
    };

    if !cli.print {
        match cli.adjtime {
            true => {
                let timeval_delta = delta.timeval();

                let ret = unsafe { libc::adjtime(&timeval_delta, std::ptr::null_mut()) };
                if ret != 0 {
                    bail!(
                        "Failed to set time with adjtime: {}",
                        std::io::Error::last_os_error()
                    );
                }
            }
            false => {
                let new_time = chrono::Utc::now() + delta;
                let new_tv = libc::timeval {
                    tv_sec: new_time.timestamp(),
                    tv_usec: new_time.timestamp_subsec_micros() as libc::suseconds_t,
                };

                let ret = unsafe { libc::settimeofday(&new_tv, std::ptr::null()) };
                if ret != 0 {
                    bail!(
                        "Failed to set time with settimeofday: {}",
                        std::io::Error::last_os_error()
                    );
                }
            }
        };
    }
    Ok(())
}
