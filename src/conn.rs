use std::io::Read;
use std::net::TcpStream;

/// State machine for parsing requests
enum RequestParser {
    /// We're ready for a new request
    Ready,
}
impl RequestParser {
    fn feed(&mut self, buf: &mut [u8]) {}
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
            parser: RequestParser::Ready,
        }
    }
    /// Poll our connection for available data, return `true` if the connection has closed
    pub fn poll(&mut self) -> bool {
        // read as many bytes as we can, up to our buffer size
        let read = match self.stream.read(&mut self.buf) {
            Ok(len) => len,
            Err(err) => {
                eprintln!("Failed to read from TCP stream: {err}");
                return true; // on failure, close the connection
            }
        };
        self.parser.feed(&mut self.buf);
        false
    }
}
