use simple_dns::{CLASS, Name, Packet, QCLASS, QTYPE, Question, RCODE, TYPE, rdata::RData};

use crate::error::{Error, Result};

use super::error::DnsError;

/// Which DNS record type to query.
#[derive(Debug, Clone, Copy)]
pub(crate) enum RecordType {
    Txt,
    Cname,
}

/// Build a DNS query packet. Returns (query_id, serialized_bytes).
pub(crate) fn build_query(domain: &str, record_type: RecordType) -> Result<(u16, Vec<u8>)> {
    let id: u16 = (rand::random::<u16>()) | 1; // avoid id=0
    let mut packet = Packet::new_query(id);

    let name = Name::new(domain)
        .map_err(|_| Error::bad_request(format!("invalid domain name: {domain}")))?;

    let qtype = match record_type {
        RecordType::Txt => QTYPE::TYPE(TYPE::TXT),
        RecordType::Cname => QTYPE::TYPE(TYPE::CNAME),
    };

    packet
        .questions
        .push(Question::new(name, qtype, QCLASS::CLASS(CLASS::IN), false));

    let bytes = packet
        .build_bytes_vec()
        .map_err(|_| Error::internal("failed to build dns query packet"))?;

    Ok((id, bytes))
}

/// Validate a DNS response: parse, check ID, check RCODE.
/// Returns the parsed packet on success.
/// NXDOMAIN (NameError) returns Ok with an empty answers section.
pub(crate) fn validate_response(data: &[u8], expected_id: u16) -> Result<Packet<'_>> {
    let packet = Packet::parse(data).map_err(|_| {
        Error::bad_gateway("dns response malformed")
            .chain(DnsError::Malformed)
            .with_code(DnsError::Malformed.code())
    })?;

    if packet.id() != expected_id {
        return Err(Error::bad_gateway("dns response id mismatch")
            .chain(DnsError::Malformed)
            .with_code(DnsError::Malformed.code()));
    }

    match packet.rcode() {
        RCODE::NoError | RCODE::NameError => Ok(packet),
        RCODE::ServerFailure => Err(Error::bad_gateway("dns server failure")
            .chain(DnsError::ServerFailure)
            .with_code(DnsError::ServerFailure.code())),
        RCODE::Refused => Err(Error::bad_gateway("dns query refused")
            .chain(DnsError::Refused)
            .with_code(DnsError::Refused.code())),
        _ => Err(Error::bad_gateway("dns query failed")
            .chain(DnsError::ServerFailure)
            .with_code(DnsError::ServerFailure.code())),
    }
}

/// Extract all TXT record strings from a parsed response packet.
///
/// `simple-dns` TXT `attributes()` returns `HashMap<String, Option<String>>`.
/// For plain verification tokens (not key=value), the token is the key with `None` value.
/// For key=value pairs, both key and value are present.
/// We collect all keys (which represent the text content of each TXT record).
pub(crate) fn extract_txt_records(packet: &Packet<'_>) -> Vec<String> {
    let mut results = Vec::new();
    for answer in &packet.answers {
        if let RData::TXT(txt) = &answer.rdata {
            for (key, value) in txt.attributes() {
                match value {
                    Some(val) => results.push(format!("{key}={val}")),
                    None => results.push(key),
                }
            }
        }
    }
    results
}

/// Extract the CNAME target from a parsed response packet (first CNAME answer).
/// CNAME is a tuple struct: `CNAME(pub Name<'a>)`.
pub(crate) fn extract_cname_target(packet: &Packet<'_>) -> Option<String> {
    for answer in &packet.answers {
        if let RData::CNAME(cname) = &answer.rdata {
            return Some(cname.0.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_txt_query_roundtrips() {
        let (id, bytes) = build_query("example.com", RecordType::Txt).unwrap();
        let packet = Packet::parse(&bytes).unwrap();
        assert_eq!(packet.id(), id);
        assert_eq!(packet.questions.len(), 1);
        assert_eq!(packet.questions[0].qname.to_string(), "example.com");
        assert_eq!(packet.questions[0].qtype, QTYPE::TYPE(TYPE::TXT));
        assert_eq!(packet.questions[0].qclass, QCLASS::CLASS(CLASS::IN));
    }

    #[test]
    fn build_cname_query_roundtrips() {
        let (id, bytes) = build_query("example.com", RecordType::Cname).unwrap();
        let packet = Packet::parse(&bytes).unwrap();
        assert_eq!(packet.id(), id);
        assert_eq!(packet.questions[0].qtype, QTYPE::TYPE(TYPE::CNAME));
    }

    #[test]
    fn parse_rcode_noerror() {
        let packet = Packet::new_query(1);
        let bytes = packet.build_bytes_vec().unwrap();
        let parsed = Packet::parse(&bytes).unwrap();
        assert_eq!(parsed.rcode(), RCODE::NoError);
    }

    #[test]
    fn id_mismatch_returns_error() {
        let (_, query_bytes) = build_query("example.com", RecordType::Txt).unwrap();
        let result = validate_response(&query_bytes, 9999);
        assert!(result.is_err());
    }
}
