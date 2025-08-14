use std::io::Read;
use std::net::TcpStream;

#[derive(PartialEq)]
enum Never {}

enum UrlEncoded {
    None,
    Percent,
    OneDigit(u8),
}

fn eat_path(
    mut buf: &mut [u8],
    in_query: &mut bool,
    encoded: &mut UrlEncoded,
) -> Option<(usize, bool)> {
    let mut eaten = 0;
    let mut to_write = matches!(encoded, UrlEncoded::Percent | UrlEncoded::OneDigit(_))
        .then_some(buf.split_off_first_mut())
        .flatten();
    while let Some(ch) = buf.split_off_first_mut() {
        // get the first character and shrink the buffer
        match *encoded {
            UrlEncoded::None => {
                eaten += 1;
                match *ch {
                    b' ' => return Some((eaten - 1, true)),
                    b'?' if !*in_query => {
                        *in_query = true;
                    }
                    b'&' | b'=' if *in_query => {}
                    b'%' => {
                        *encoded = UrlEncoded::Percent;
                        to_write = Some(ch);
                    }
                    b => {
                        if !(b.is_ascii_alphanumeric() || b"/-_.~:#[]!$()*+,;".contains(&b)) {
                            return None;
                        }
                    }
                }
            }
            UrlEncoded::Percent => {
                if let Some(d) = (*ch as char).to_digit(16) {
                    *encoded = UrlEncoded::OneDigit(d as _);
                } else {
                    return None;
                }
            }
            UrlEncoded::OneDigit(d1) => {
                if let Some(d2) = (*ch as char).to_digit(16) {
                    *to_write.take().unwrap() = (d1 << 4) | d2 as u8;
                } else {
                    return None;
                }
            }
        }
    }
    Some((eaten, false))
}

enum HttpSeen {
    None,
    Dot,
    Cr,
}

/// State machine for parsing requests
enum RequestParser {
    /// We're waiting on a method, with a number of parsed bytes
    Method {
        method_end: usize,
    },
    Path {
        method_end: usize,
        path_end: usize,
        in_query: bool,
        encoded: UrlEncoded,
    },
    Http {
        method_end: usize,
        path_end: usize,
        http_end: usize,
        seen: HttpSeen,
    },
    Headers,
}
impl RequestParser {
    /// Space in the buffer that we're using for scratch
    fn used_space(&self) -> usize {
        match *self {
            Self::Method { method_end } => method_end,
            Self::Path { path_end, .. } => path_end,
            Self::Http { http_end, .. } => http_end,
            Self::Headers => 0,
        }
    }
    /// Accept incoming data
    ///
    /// The data will start at buf[self.used_space()] and have a length of read.
    /// Returns Err(true) on invalid data, or Err(false) on incomplete data
    fn feed(&mut self, buf: &mut [u8], mut read: usize) -> Result<(), bool> {
        while read > 0 {
            match self.feed_step(buf, &mut read) {
                Err(false) => {}
                Err(true) => return Err(true),
            }
        }
        Err(false)
    }
    fn feed_step(&mut self, buf: &mut [u8], read: &mut usize) -> Result<Never, bool> {
        match self {
            Self::Method { method_end } => {
                let end = *method_end + *read;
                let avail = &mut buf[*method_end..end];
                let res = avail.iter_mut().enumerate().find_map(|(n, b)| {
                    if b.is_ascii_uppercase() {
                        None
                    } else if b.is_ascii_lowercase() {
                        *b -= 0x20; // make uppercase
                        None
                    } else if *b == b' ' {
                        Some(Ok(n))
                    } else {
                        Some(Err(()))
                    }
                });
                match res {
                    Some(Ok(idx)) => {
                        let path_end = *method_end + idx;
                        *read -= idx;
                        buf[path_end..end].rotate_left(1); // remove the space
                        *self = Self::Path {
                            method_end: path_end,
                            path_end,
                            in_query: false,
                            encoded: UrlEncoded::None,
                        };
                    }
                    Some(Err(_)) => {
                        return Err(true);
                    }
                    None => *method_end += *read,
                }
            }
            Self::Path {
                method_end,
                path_end,
                in_query,
                encoded,
            } => {
                let end = *path_end + *read;
                let res = eat_path(&mut buf[(*path_end - 1)..end], in_query, encoded);
                if let Some((eaten, done)) = res {
                    *path_end += eaten;
                    *read -= eaten;
                    if done {
                        buf[*path_end..end].rotate_left(1);
                        *self = Self::Http {
                            method_end: *method_end,
                            path_end: *path_end,
                            http_end: *path_end,
                            seen: HttpSeen::None,
                        };
                    }
                } else {
                    return Err(true);
                }
            }
            Self::Http {
                path_end,
                http_end,
                seen,
                ..
            } => {
                // we need to see HTTP/1[.x]\r\n
                let seen_bytes = *http_end - *path_end;
                let end = *http_end + *read;
                let mut avail = &buf[*http_end..end];
                if let Some(rem) = b"HTTP/1".get(seen_bytes..) {
                    if rem.len() < *read {
                        if let Some(rest) = avail.strip_prefix(rem) {
                            *http_end += rem.len();
                            avail = rest;
                        } else {
                            return Err(true);
                        }
                    } else {
                        if rem.starts_with(avail) {
                            *http_end += avail.len();
                            avail = &[];
                        } else {
                            return Err(true);
                        }
                    }
                }
                while let Some(ch) = avail.split_off_first() {
                    match seen {
                        HttpSeen::None => {
                            if *ch == b'.' {
                                *seen = HttpSeen::Dot;
                            } else {
                                return Err(true);
                            }
                        }
                        HttpSeen::Dot => {
                            if *ch == b'\r' {
                                *seen = HttpSeen::Cr;
                            } else if !ch.is_ascii_digit() {
                                return Err(true);
                            }
                        }
                        HttpSeen::Cr => {
                            if *ch == b'\n' {
                                *self = Self::Headers;
                                break;
                            } else {
                                return Err(true);
                            }
                        }
                    }
                }
            }
            Self::Headers => todo!(),
        }
        Err(false)
    }
}
impl Default for RequestParser {
    fn default() -> Self {
        Self::Method { method_end: 0 }
    }
}

/// A wrapper around an open connection
pub struct Connection {
    stream: TcpStream,
    buf: [u8; 512],
    parser: RequestParser,
}
impl Connection {
    /// Create a new connection
    pub fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            buf: [0; 512],
            parser: RequestParser::default(),
        }
    }
    /// Poll our connection for available data, return `true` if the connection has closed
    pub fn poll(&mut self) -> bool {
        // read as many bytes as we can, up to our buffer size
        let read = match self.stream.read(&mut self.buf[self.parser.used_space()..]) {
            Ok(len) => len,
            Err(err) => {
                eprintln!("Failed to read from TCP stream: {err}");
                return true; // on failure, close the connection
            }
        };
        match self.parser.feed(&mut self.buf, read) {
            Ok(()) => {}
            Err(false) => {}
            Err(true) => {
                return true;
            }
        }
        false
    }
}
