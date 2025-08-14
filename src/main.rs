#![allow(clippy::collapsible_if, clippy::collapsible_else_if)]

use std::fmt::Display;
use std::net::TcpListener;
use std::process::exit;

mod conn;

/// Create a function that prints a message and an error, then exits with error code 1
fn handle_err<E: Display, T>(msg: &str) -> impl Fn(E) -> T {
    move |err| {
        eprintln!("{msg}: {err}");
        exit(1)
    }
}

const MAX_CONNECTIONS: usize = 4;

fn main() {
    let mut conns = [const { None::<conn::Connection> }; MAX_CONNECTIONS]; // we start by initializing an array for connections
    let listener = TcpListener::bind(("localhost", 8080))
        .unwrap_or_else(handle_err("Failed to bind listener")); // bind the listener, exit if it fails
    listener
        .set_nonblocking(true)
        .unwrap_or_else(handle_err("Failed to config listener")); // we want a nonblocking listener
    for incoming in listener.incoming() {
        let mut it = conns.iter_mut();
        // if we have a new connection, try to put it in our connection array
        if let Ok(incoming) = incoming {
            for opt in &mut it {
                if let Some(c2) = opt {
                    // we poll until we find a slot and if possible, insert it
                    if c2.poll() {
                        *c2 = conn::Connection::new(incoming);
                        break;
                    }
                } else {
                    *opt = Some(conn::Connection::new(incoming));
                    break;
                }
            }
        }
        // then we poll the rest and remove them from the connection if necessary
        for opt in it {
            if let Some(conn) = opt {
                if conn.poll() {
                    *opt = None;
                }
            }
        }
    }
}
