//! NTLMv2 (MS-NLMP): negotiate, then authenticate with the server's challenge.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use hmac::{Hmac, Mac};
use md4::Digest;
use md5::Md5;

const FLAGS: u32 = 0x0000_0001 // UNICODE
    | 0x0000_0002 // OEM
    | 0x0000_0004 // REQUEST_TARGET
    | 0x0000_0200 // NTLM
    | 0x0000_8000 // ALWAYS_SIGN
    | 0x0008_0000 // EXTENDED_SESSIONSECURITY
    | 0x0080_0000 // TARGET_INFO
    | 0x2000_0000 // 128
    | 0x8000_0000; // 56

fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(|c| c.to_le_bytes()).collect()
}

fn hmac_md5(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Md5>::new_from_slice(key).expect("any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// `NTLM <base64>` negotiate message.
pub fn negotiate() -> String {
    let mut msg = b"NTLMSSP\0".to_vec();
    msg.extend_from_slice(&1u32.to_le_bytes());
    msg.extend_from_slice(&FLAGS.to_le_bytes());
    msg.extend_from_slice(&[0; 16]); // empty domain and workstation
    format!("NTLM {}", STANDARD.encode(msg))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Challenge {
    pub server_challenge: [u8; 8],
    pub target_info: Vec<u8>,
}

/// Parses `NTLM <base64>` (type 2) among `WWW-Authenticate` values.
pub fn parse(values: &[String]) -> Option<Challenge> {
    let msg = values.iter().find_map(|v| {
        v.trim()
            .strip_prefix("NTLM ")
            .and_then(|b| STANDARD.decode(b.trim()).ok())
    })?;
    if !msg.starts_with(b"NTLMSSP\0") || msg.get(8) != Some(&2) || msg.len() < 32 {
        return None;
    }
    let server_challenge: [u8; 8] = msg[24..32].try_into().ok()?;
    let target_info = if msg.len() >= 48 {
        let len = u16::from_le_bytes([msg[40], msg[41]]) as usize;
        let off = u32::from_le_bytes(msg[44..48].try_into().ok()?) as usize;
        msg.get(off..off + len)
            .map(<[u8]>::to_vec)
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    Some(Challenge {
        server_challenge,
        target_info,
    })
}

/// NTOWFv2 = HMAC_MD5(MD4(UNICODE(password)), UNICODE(UPPER(user) + domain)).
pub fn ntowf_v2(user: &str, password: &str, domain: &str) -> Vec<u8> {
    let nt_hash = md4::Md4::digest(utf16(password));
    hmac_md5(
        &nt_hash,
        &utf16(&format!("{}{domain}", user.to_uppercase())),
    )
}

/// `temp` blob of the NTLMv2 response.
fn blob(timestamp: u64, client_challenge: &[u8; 8], target_info: &[u8]) -> Vec<u8> {
    let mut temp = vec![1, 1, 0, 0, 0, 0, 0, 0];
    temp.extend_from_slice(&timestamp.to_le_bytes());
    temp.extend_from_slice(client_challenge);
    temp.extend_from_slice(&[0; 4]);
    temp.extend_from_slice(target_info);
    temp.extend_from_slice(&[0; 4]);
    temp
}

/// (NtChallengeResponse, LmChallengeResponse).
pub fn responses(
    key: &[u8],
    challenge: &Challenge,
    client_challenge: &[u8; 8],
    timestamp: u64,
) -> (Vec<u8>, Vec<u8>) {
    let temp = blob(timestamp, client_challenge, &challenge.target_info);
    let mut data = challenge.server_challenge.to_vec();
    data.extend_from_slice(&temp);
    let mut nt = hmac_md5(key, &data);
    nt.extend_from_slice(&temp);
    let mut lm_data = challenge.server_challenge.to_vec();
    lm_data.extend_from_slice(client_challenge);
    let mut lm = hmac_md5(key, &lm_data);
    lm.extend_from_slice(client_challenge);
    (nt, lm)
}

/// `NTLM <base64>` authenticate message. `user` may be `DOMAIN\user`.
pub fn authenticate(
    user: &str,
    password: &str,
    challenge: &Challenge,
    client_challenge: &[u8; 8],
    unix_secs: i64,
) -> String {
    let (domain, name) = match user.split_once('\\') {
        Some((d, n)) => (d, n),
        None => ("", user),
    };
    // Windows FILETIME: 100 ns intervals since 1601-01-01.
    let timestamp = (unix_secs.max(0) as u64 + 11_644_473_600) * 10_000_000;
    let key = ntowf_v2(name, password, domain);
    let (nt, lm) = responses(&key, challenge, client_challenge, timestamp);
    let (domain, name) = (utf16(domain), utf16(name));
    let mut payload: Vec<u8> = Vec::new();
    let mut header: Vec<u8> = b"NTLMSSP\0".to_vec();
    header.extend_from_slice(&3u32.to_le_bytes());
    let base = 64u32;
    let mut field = |data: &[u8], header: &mut Vec<u8>| {
        let off = base + payload.len() as u32;
        header.extend_from_slice(&(data.len() as u16).to_le_bytes());
        header.extend_from_slice(&(data.len() as u16).to_le_bytes());
        header.extend_from_slice(&off.to_le_bytes());
        payload.extend_from_slice(data);
    };
    field(&lm, &mut header);
    field(&nt, &mut header);
    field(&domain, &mut header);
    field(&name, &mut header);
    field(&[], &mut header); // workstation
    field(&[], &mut header); // session key
    header.extend_from_slice(&FLAGS.to_le_bytes());
    header.extend_from_slice(&payload);
    format!("NTLM {}", STANDARD.encode(header))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace()
            .map(|b| u8::from_str_radix(b, 16).unwrap())
            .collect()
    }

    /// MS-NLMP 4.2.4 (NTLMv2 authentication).
    #[test]
    fn ms_nlmp_vectors() {
        let key = ntowf_v2("User", "Password", "Domain");
        assert_eq!(key, hex("0c 86 8a 40 3b fd 7a 93 a3 00 1e f2 2e f0 2e 3f"));
        let target_info = hex(
            "02 00 0c 00 44 00 6f 00 6d 00 61 00 69 00 6e 00 01 00 0c 00 53 00 65 00 72 00 76 00 65 00 72 00 00 00 00 00",
        );
        let challenge = Challenge {
            server_challenge: [1, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef],
            target_info,
        };
        let (nt, lm) = responses(&key, &challenge, &[0xaa; 8], 0);
        assert_eq!(
            nt[..16],
            hex("68 cd 0a b8 51 e5 1c 96 aa bc 92 7b eb ef 6a 1c")[..]
        );
        assert_eq!(
            lm[..16],
            hex("86 c3 50 97 ac 9c ec 10 25 54 76 4a 57 cc cc 19")[..]
        );
    }
}
