use sha1::Sha1;
use websocket::{Frame, FrameKind};
use http::HttpRequest;

use std::io::{Write};
use std::fmt;

use crate::base64::to_base64;
use crate::http_handler::get_response_to_http;
use crate::log;
use crate::god_set::GodSet;
use crate::filler;
use crate::json::Json;
use std::str::FromStr;
use std::net::TcpStream;

const WEBSOCKET_SECURE_KEY_MAGIC_NUMBER: &'static str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

pub struct ServerState {
    clients: Vec<Client>,
    id_generator: ClientIdGenerator,
    god_set: GodSet,
}

impl ServerState {
    pub fn new() -> ServerState {
        match GodSet::new() {
            Some(god_set) => ServerState { clients: Vec::new(), id_generator: ClientIdGenerator::new(), god_set },
            None => {
                log!("Couldn't parse godset file");
                panic!();
            }
        }
    }

    pub fn new_connection_handler(&mut self, stream: TcpStream) -> ClientId {
        let id = self.id_generator.next();

        match stream.peer_addr() {
            Ok(addr) => log!("new connection to {}:{}, id {}", addr.ip(), addr.port(), id),
            Err(_) => log!("new connection to unknown, id {}", id),
        };

        self.clients.push(Client::new(id, stream));

        id
    }

    pub fn drop_handler(&mut self, id: ClientId) {
        log!("dropped connection to {}", id);

        if let Some((i, _)) = self.clients.iter().enumerate().find(|(_, c)| c.id == id) {
            self.clients.remove(i);
        }
    }

    pub fn websocket_message_handler(&mut self, id: ClientId, message_bytes: Vec<u8>, kind: FrameKind) -> StreamState {
        match kind {
            FrameKind::Text => self.text_websocket_message_handler(id, message_bytes),
            FrameKind::Binary => {
                // received unexpectedly
                // TODO: add support
                StreamState::Drop
            },
            FrameKind::Ping => {
                let pong_frame = Frame::from_payload(FrameKind::Pong, message_bytes).encode();
                self.write_bytes_to(id, &pong_frame)
            },
            FrameKind::Continue => {
                log!("panicking, did not expect continue message from {}", id);
                StreamState::Drop
            },
            FrameKind::Pong => StreamState::Keep,
            FrameKind::Close => StreamState::Drop, // that's what they want us to do anyway, right?
        }
    }

    pub fn text_websocket_message_handler(&mut self, id: ClientId, message_bytes: Vec<u8>) -> StreamState {
        match self.get_client(id).unwrap().upgraded.as_ref().unwrap().resource_location() {
            "/godset" => {
                let response_frame = Frame::from_payload(FrameKind::Text, self.god_set.raw_bytes()).encode();
                self.write_bytes_to(id, &response_frame)
            },
            "/filler" => {
                // var a = new WebSocket("ws://ethan.ws/filler"); a.onmessage = function(msg) {console.log(msg)}
                match String::from_utf8(message_bytes) {
                    Ok(string) => match Json::from_str(&string) {
                        Ok(ref json) if json.get_null().is_some() => {
                            let json_string = filler::new_game_state().to_string();
                            let frame = Frame::from_payload(FrameKind::Text, json_string.into_bytes());
                            self.write_bytes_to(id, &frame.encode())
                        },
                        Ok(ref json) => match filler::handle_request(json) {
                            Some(reply) => {
                                let frame = Frame::from_payload(FrameKind::Text, reply.to_string().into_bytes());
                                self.write_bytes_to(id, &frame.encode())
                            },
                            None => StreamState::Drop,
                        },
                        Err(_) => StreamState::Drop,
                    },
                    Err(_) => StreamState::Drop,
                }
            },
            _ => StreamState::Drop
        }
    }

    pub fn http_message_handler(&mut self, id: ClientId, message: HttpRequest) -> StreamState {
        log!("client {} requested {}", id, message.resource_location());

        // return StreamState::Keep if our connection should be updated to websockets
        match self.get_writer(id) {
            Some(writer) => match handle_deelio(&message) {
                Some(websocket_upgrade_response) => {
                    match writer.write_all(websocket_upgrade_response.as_bytes()) {
                        Ok(_) => {
                            self.get_client_mut(id).unwrap().upgrade(message);
                            StreamState::Keep
                        },
                        Err(_) => StreamState::Drop,
                    }
                },
                None => {
                    let _ = get_response_to_http(&message, writer);
                    StreamState::Drop
                },
            },
            None => {
                log!("Client {} should exist", id);
                StreamState::Drop
            },
        }
    }

    fn write_bytes_to(&mut self, id: ClientId, bytes: &[u8]) -> StreamState {
        match self.get_client_mut(id) {
            Some(c) => match c.writer.write_all(bytes) {
                Ok(_) => StreamState::Keep,
                Err(_) => StreamState::Drop,
            },
            None => StreamState::Drop,
        }
    }

    fn get_client(&self, id: ClientId) -> Option<&Client> {
        self.clients.iter()
            .find(|c| c.id == id)
    }

    fn get_client_mut(&mut self, id: ClientId) -> Option<&mut Client> {
        self.clients.iter_mut()
            .find(|c| c.id == id)
    }

    fn get_writer(&mut self, id: ClientId) -> Option<&mut TcpStream> {
        self.get_client_mut(id).map(|c| &mut c.writer)
    }
}


pub struct Client {
    pub id: ClientId,
    pub upgraded: Option<HttpRequest>, // the request that they used to upgrade their connection
    pub writer: TcpStream,
}

impl Client {
    fn new(id: ClientId, writer: TcpStream) -> Client {
        Client { id, writer, upgraded: None }
    }

    fn upgrade(&mut self, message: HttpRequest) {
        self.upgraded = Some(message);
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct ClientId(u64);

impl fmt::Display for ClientId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

struct ClientIdGenerator(u64);
impl ClientIdGenerator {
    fn new() -> ClientIdGenerator {
        ClientIdGenerator(0)
    }

    fn next(&mut self) -> ClientId {
        self.0 += 1;
        ClientId(self.0)
    }
}


pub enum StreamState {
    Keep,
    Drop,
}

fn handle_deelio(request: &HttpRequest) -> Option<String> {
    if request.get_header_value("Upgrade") == Some("websocket") {
        match request.get_header_value("Sec-WebSocket-Key") {
            Some(key) => {
                if !key.is_ascii() { return None };
                let to_hash = format!("{}{}", key, WEBSOCKET_SECURE_KEY_MAGIC_NUMBER);
                let response = to_base64(&Sha1::from(to_hash.as_bytes()).digest().bytes());
                // NOTE: excludes header: nSec-WebSocket-Protocol: chat
                Some(format!("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n", response))
            },
            None => None,
        }
    } else {
        None
    }
}
