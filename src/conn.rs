use std::net::TcpStream;

pub struct Connection {
    stream: TcpStream,
}
impl Connection {
    pub fn new(stream: TcpStream) -> Self {
        Self { stream }
    }
    pub fn poll(&mut self) -> bool {
        false
    }
}
