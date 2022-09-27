# rrdate

A tool for querying the current time from a network server and setting the system time written in Rust.
rrdate implements both Simple Network Time Protocol ([SNTP](https://datatracker.ietf.org/doc/html/rfc4330)) and [Time Protocol](https://datatracker.ietf.org/doc/html/rfc868)  clients, and provides the `rdate` command. Unlike traditional `rdate` implementations, rrdate defaults to using NTP, which is more accurate than Time Protocol.

## Installing

```
cargo install --git https://github.com/guyru/rrdate
```

Setting time requires running the program as root. Alternatively, you can add the `CAP_SYS_TIME` capability to the `rdate` binary:

```
sudo setcap cap_sys_time+ep ./rdate
```

This will allow using the program as normal user.

## Usage

```
    rdate [OPTIONS] <HOST>

ARGS:
    <HOST>    Time server (e.g. time.nist.gov)

OPTIONS:
    -a                   Use the adjtime(2) call to gradually skew the local time to the remote time
                         rather than just hopping
    -h, --help           Print help information
    -o, --port <PORT>    Use port 'port' instead of port 37 (RFC 868) or 123 (SNTP, RFC 5905)
    -p, --print          Just print, don't set
        --rfc868         Use RFC 868 time protocol instead of SNTP (RFC 5905)
    -s, --silent         Just set, don't print
    -u                   Use UDP instead of TCP as transport for RFC 868. SNTP will always use UDP
                         protocol
    -v, --verbose        Verbose output
    -V, --version        Print version information

```

## Examples

Query and print the time:

```
# rdate -p 0.debian.pool.ntp.org
Tue Sep 27 09:50:30 2022

```

Gradually correct the time:
```
# rdate -va 0.debian.pool.ntp.org
Precision: 1μs (-21)
Jitter: 52.5μs
Delay: 2ms
Tue Sep 27 09:50:30 2022
adjust local clock by 0.003534 seconds (adjtime)
```



## License

Copyright (C) 2022  Guy Rutenberg

This program is free software; you can redistribute it and/or
modify it under the terms of the GNU General Public License
as published by the Free Software Foundation; either version 2
of the License, or (at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with this program; if not, write to the Free Software
Foundation, Inc., 51 Franklin Street, Fifth Floor, Boston, MA  02110-1301, USA.

## Authors
- Author: [Guy Rutenberg](https://www.guyrutenberg.com)
