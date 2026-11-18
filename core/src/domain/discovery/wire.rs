//! The part of the DNS wire format a node's resolver speaks: one question
//! per message in, and an answer with address records, or a failure code,
//! out. Everything else a client sends is passed to the host's resolvers as
//! the bytes it arrived in.

use std::net::Ipv4Addr;

/// An IPv4 address record.
pub const TYPE_A: u16 = 1;
/// Any record of the name.
pub const TYPE_ANY: u16 = 255;
/// The Internet class, the only one names live in here.
pub const CLASS_IN: u16 = 1;
/// The largest answer a client that announced nothing else accepts in a
/// datagram.
pub const DATAGRAM_LIMIT: usize = 512;
/// The largest message of the stream form, which a two-byte length frames.
pub const STREAM_LIMIT: usize = 65_535;

const HEADER: usize = 12;
const FLAG_RESPONSE: u8 = 0x80;
const FLAG_AUTHORITATIVE: u8 = 0x04;
const FLAG_TRUNCATED: u8 = 0x02;
const FLAG_RECURSION_DESIRED: u8 = 0x01;
const FLAG_RECURSION_AVAILABLE: u8 = 0x80;
/// One address record: a pointer to the question's name, type, class, time
/// to live, length, and the four bytes of the address.
const ADDRESS_RECORD: usize = 16;

/// Why a message is answered with a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// The message is not a well-formed question.
    Format,
    /// The host's resolvers did not answer.
    Server,
    /// The host's resolvers do not resolve the name.
    NoSuchName,
    /// Not a standard query.
    NotImplemented,
}

impl Failure {
    const fn code(self) -> u8 {
        match self {
            Self::Format => 1,
            Self::Server => 2,
            Self::NoSuchName => 3,
            Self::NotImplemented => 4,
        }
    }
}

/// The one question of a query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub id: u16,
    /// The name in lower case without the final dot. A byte that cannot be
    /// part of a host name is replaced, so such a name matches nothing.
    pub name: String,
    pub kind: u16,
    pub class: u16,
    recursion_desired: bool,
    /// The question as it was sent: an answer repeats it.
    section: Vec<u8>,
}

/// What a received message is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Query {
    Question(Question),
    /// A query to answer with a failure.
    Refused {
        reply: Vec<u8>,
    },
    /// Not a query at all: nothing is sent back.
    Noise,
}

/// Reads a query. Only its header and question are looked at; additional
/// records, such as the one that announces a larger datagram, stay with the
/// bytes when the query is passed on.
#[must_use]
pub fn parse(message: &[u8]) -> Query {
    let Some(header) = message.get(..HEADER) else {
        return Query::Noise;
    };
    let id = u16::from_be_bytes([header[0], header[1]]);
    if header[2] & FLAG_RESPONSE != 0 {
        return Query::Noise;
    }
    let recursion_desired = header[2] & FLAG_RECURSION_DESIRED != 0;
    let refuse = |failure| Query::Refused {
        reply: reply_header(id, recursion_desired, failure_flags(failure), 0, 0),
    };
    if (header[2] >> 3) & 0x0f != 0 {
        return refuse(Failure::NotImplemented);
    }
    if u16::from_be_bytes([header[4], header[5]]) != 1 {
        return refuse(Failure::Format);
    }
    let Some((name, end)) = read_name(message, HEADER) else {
        return refuse(Failure::Format);
    };
    let Some(fixed) = message.get(end..end + 4) else {
        return refuse(Failure::Format);
    };
    Query::Question(Question {
        id,
        name,
        kind: u16::from_be_bytes([fixed[0], fixed[1]]),
        class: u16::from_be_bytes([fixed[2], fixed[3]]),
        recursion_desired,
        section: message[HEADER..end + 4].to_vec(),
    })
}

/// The name that starts at `start`, and where it ends. A question holds its
/// name in full: a pointer into the message is refused.
fn read_name(message: &[u8], start: usize) -> Option<(String, usize)> {
    let mut name = String::new();
    let mut position = start;
    loop {
        let length = usize::from(*message.get(position)?);
        position += 1;
        if length == 0 {
            return Some((name, position));
        }
        if length > 63 || position + length - start > 255 {
            return None;
        }
        let label = message.get(position..position + length)?;
        if !name.is_empty() {
            name.push('.');
        }
        name.extend(label.iter().map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_') {
                char::from(byte.to_ascii_lowercase())
            } else {
                char::REPLACEMENT_CHARACTER
            }
        }));
        position += length;
    }
}

const fn failure_flags(failure: Failure) -> u8 {
    FLAG_RECURSION_AVAILABLE | failure.code()
}

fn reply_header(
    id: u16,
    recursion_desired: bool,
    low: u8,
    questions: u16,
    answers: u16,
) -> Vec<u8> {
    let mut reply = Vec::with_capacity(HEADER);
    reply.extend_from_slice(&id.to_be_bytes());
    reply.push(
        FLAG_RESPONSE
            | if recursion_desired {
                FLAG_RECURSION_DESIRED
            } else {
                0
            },
    );
    reply.push(low);
    reply.extend_from_slice(&questions.to_be_bytes());
    reply.extend_from_slice(&answers.to_be_bytes());
    reply.extend_from_slice(&[0; 4]);
    reply
}

impl Question {
    /// Whether the question asks for IPv4 addresses.
    #[must_use]
    pub const fn wants_addresses(&self) -> bool {
        self.class == CLASS_IN && (self.kind == TYPE_A || self.kind == TYPE_ANY)
    }

    /// The answer for a name this node knows: `addresses` when the question
    /// asks for addresses, and an empty answer for every other record type,
    /// which tells the client the name exists and has no such record.
    /// Addresses that do not fit in `limit` bytes are left out and the
    /// answer is marked truncated, so the client asks again over a stream.
    #[must_use]
    pub fn answer(&self, addresses: &[Ipv4Addr], time_to_live: u32, limit: usize) -> Vec<u8> {
        let addresses = if self.wants_addresses() {
            addresses
        } else {
            &[]
        };
        let room = limit.saturating_sub(HEADER + self.section.len()) / ADDRESS_RECORD;
        let count = addresses.len().min(room).min(usize::from(u16::MAX));
        let mut reply = reply_header(
            self.id,
            self.recursion_desired,
            FLAG_RECURSION_AVAILABLE,
            1,
            u16::try_from(count).unwrap_or(u16::MAX),
        );
        reply[2] |= FLAG_AUTHORITATIVE;
        if count < addresses.len() {
            reply[2] |= FLAG_TRUNCATED;
        }
        reply.extend_from_slice(&self.section);
        for address in &addresses[..count] {
            // The name is the question's, twelve bytes into the message.
            reply.extend_from_slice(&[0xc0, 0x0c]);
            reply.extend_from_slice(&TYPE_A.to_be_bytes());
            reply.extend_from_slice(&CLASS_IN.to_be_bytes());
            reply.extend_from_slice(&time_to_live.to_be_bytes());
            reply.extend_from_slice(&4_u16.to_be_bytes());
            reply.extend_from_slice(&address.octets());
        }
        reply
    }

    /// A failure answer that repeats the question.
    #[must_use]
    pub fn failure(&self, failure: Failure) -> Vec<u8> {
        let mut reply = reply_header(
            self.id,
            self.recursion_desired,
            failure_flags(failure),
            1,
            0,
        );
        reply.extend_from_slice(&self.section);
        reply
    }
}

/// The identifier of a message, which pairs an answer with its query.
#[must_use]
pub fn message_id(message: &[u8]) -> Option<u16> {
    message
        .get(..2)
        .map(|bytes| u16::from_be_bytes([bytes[0], bytes[1]]))
}

/// `message` with its identifier replaced.
#[must_use]
pub fn with_message_id(message: &[u8], id: u16) -> Vec<u8> {
    let mut changed = message.to_vec();
    if let Some(bytes) = changed.get_mut(..2) {
        bytes.copy_from_slice(&id.to_be_bytes());
    }
    changed
}

/// Whether `message` is an answer in which a resolver refuses to resolve
/// the name, as a host's stub resolver does for a name without a dot.
#[must_use]
pub fn is_refusal(message: &[u8]) -> bool {
    is_response(message) && message.get(3).is_some_and(|flags| flags & 0x0f == 5)
}

/// Whether `message` is an answer.
#[must_use]
pub fn is_response(message: &[u8]) -> bool {
    message
        .get(2)
        .is_some_and(|flags| flags & FLAG_RESPONSE != 0)
}
